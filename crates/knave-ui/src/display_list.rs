use crate::{
    Color, ImageStyle, Rect, ShapePaint, TextStyle, Transform2D, UiImage, UiNode, UiScene,
};

/// One renderer-independent drawing operation in back-to-front order.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommand {
    RichText {
        bounds: Rect,
        spans: Vec<crate::TextSpan>,
        style: TextStyle,
        transform: Transform2D,
        clip: Option<Rect>,
    },
    Shape {
        bounds: Rect,
        paint: ShapePaint,
        transform: Transform2D,
        clip: Option<Rect>,
    },
    Text {
        bounds: Rect,
        style: TextStyle,
        text: String,
        transform: Transform2D,
        clip: Option<Rect>,
    },
    Image {
        bounds: Rect,
        image: UiImage,
        style: ImageStyle,
        transform: Transform2D,
        clip: Option<Rect>,
    },
}

/// An immutable, ordered paint description produced by a UI scene.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayList {
    pub revision: u64,
    pub clear_color: Color,
    pub commands: Vec<DisplayCommand>,
}

impl DisplayList {
    pub fn new(revision: u64, clear_color: Color) -> Self {
        Self {
            revision,
            clear_color,
            commands: Vec::new(),
        }
    }

    pub fn from_scene(scene: &UiScene) -> Self {
        let mut builder = DisplayListBuilder::new(scene.revision(), scene.clear_color());
        for node in scene.nodes() {
            match node {
                UiNode::Panel { bounds, color, .. } => {
                    builder.fill_rect(*bounds, *color);
                }
                UiNode::Label {
                    bounds,
                    color,
                    text,
                    ..
                } => {
                    builder.text(
                        *bounds,
                        text.clone(),
                        TextStyle {
                            color: *color,
                            ..TextStyle::default()
                        },
                    );
                }
                UiNode::Image { bounds, image, .. } => {
                    builder.image(*bounds, image.clone(), ImageStyle::default());
                }
            }
        }
        builder
            .finish()
            .expect("scene conversion does not alter builder state")
    }
}

impl Default for DisplayList {
    fn default() -> Self {
        Self::new(0, Color::BACKGROUND)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayListBuildError {
    UnbalancedTransforms,
    UnbalancedClips,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ClipState {
    Unbounded,
    Bounded(Rect),
    Empty,
}

/// A small stateful builder. Push/pop operations are explicit so clipping and
/// transforms stay local to the intended group of commands.
#[derive(Clone, Debug)]
pub struct DisplayListBuilder {
    list: DisplayList,
    transforms: Vec<Transform2D>,
    clips: Vec<ClipState>,
}

impl DisplayListBuilder {
    pub fn new(revision: u64, clear_color: Color) -> Self {
        Self {
            list: DisplayList::new(revision, clear_color),
            transforms: vec![Transform2D::IDENTITY],
            clips: vec![ClipState::Unbounded],
        }
    }

    pub fn push_transform(&mut self, local: Transform2D) {
        let current = *self.transforms.last().expect("root transform exists");
        self.transforms.push(current.compose(local));
    }

    pub fn pop_transform(&mut self) -> Result<(), DisplayListBuildError> {
        if self.transforms.len() == 1 {
            return Err(DisplayListBuildError::UnbalancedTransforms);
        }
        self.transforms.pop();
        Ok(())
    }

    /// Add a rectangular clip in the current local coordinate system.
    /// Rotated clips are conservatively represented by their axis-aligned bounds.
    pub fn push_clip(&mut self, bounds: Rect) {
        let current = *self.transforms.last().expect("root transform exists");
        let transformed = current.transform_rect_bounds(bounds);
        let parent = *self.clips.last().expect("root clip exists");
        let next = match parent {
            ClipState::Unbounded if transformed.is_finite_positive() => {
                ClipState::Bounded(transformed)
            }
            ClipState::Unbounded => ClipState::Empty,
            ClipState::Bounded(parent) => parent
                .intersection(transformed)
                .map(ClipState::Bounded)
                .unwrap_or(ClipState::Empty),
            ClipState::Empty => ClipState::Empty,
        };
        self.clips.push(next);
    }

    pub fn pop_clip(&mut self) -> Result<(), DisplayListBuildError> {
        if self.clips.len() == 1 {
            return Err(DisplayListBuildError::UnbalancedClips);
        }
        self.clips.pop();
        Ok(())
    }

    pub fn fill_rect(&mut self, bounds: Rect, color: Color) {
        self.shape(bounds, ShapePaint::fill(color));
    }

    pub fn shape(&mut self, bounds: Rect, paint: ShapePaint) {
        self.push_command(|transform, clip| DisplayCommand::Shape {
            bounds,
            paint,
            transform,
            clip,
        });
    }

    pub fn text(&mut self, bounds: Rect, text: String, style: TextStyle) {
        self.push_command(|transform, clip| DisplayCommand::Text {
            bounds,
            style,
            text,
            transform,
            clip,
        });
    }

    pub fn image(&mut self, bounds: Rect, image: UiImage, style: ImageStyle) {
        self.push_command(|transform, clip| DisplayCommand::Image {
            bounds,
            image,
            style,
            transform,
            clip,
        });
    }

    fn push_command(&mut self, make: impl FnOnce(Transform2D, Option<Rect>) -> DisplayCommand) {
        let Some(clip) = self.clips.last() else {
            unreachable!("root clip exists")
        };
        if *clip == ClipState::Empty {
            return;
        }
        let transform = *self.transforms.last().expect("root transform exists");
        let clip = match clip {
            ClipState::Unbounded => None,
            ClipState::Bounded(bounds) => Some(*bounds),
            ClipState::Empty => unreachable!("empty clips return above"),
        };
        self.list.commands.push(make(transform, clip));
    }

    pub fn finish(self) -> Result<DisplayList, DisplayListBuildError> {
        if self.transforms.len() != 1 {
            return Err(DisplayListBuildError::UnbalancedTransforms);
        }
        if self.clips.len() != 1 {
            return Err(DisplayListBuildError::UnbalancedClips);
        }
        Ok(self.list)
    }
}

pub type RenderCommand = DisplayCommand;
pub type RenderList = DisplayList;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CornerRadii, NodeId};

    #[test]
    fn commands_remain_in_insertion_order_and_keep_full_styles() {
        let mut builder = DisplayListBuilder::new(3, Color::BACKGROUND);
        builder.image(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            UiImage::from_rgba(1, 1, vec![255; 4]).unwrap(),
            ImageStyle::default(),
        );
        builder.shape(
            Rect::new(0.0, 0.0, 2.0, 2.0),
            ShapePaint {
                radii: CornerRadii::uniform(5.0),
                ..ShapePaint::fill(Color::ACCENT)
            },
        );
        let list = builder.finish().unwrap();
        assert!(matches!(list.commands[0], DisplayCommand::Image { .. }));
        assert!(matches!(list.commands[1], DisplayCommand::Shape { .. }));
        let DisplayCommand::Shape { paint, .. } = &list.commands[1] else {
            unreachable!()
        };
        assert_eq!(paint.radii, CornerRadii::uniform(5.0));
    }

    #[test]
    fn scene_projection_preserves_order_revision_and_clear_color() {
        let mut scene = UiScene::new(11);
        scene.set_clear_color(Color::rgba(8, 12, 18, 245));
        scene.push(UiNode::Panel {
            id: NodeId(1),
            bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
            color: Color::ACCENT,
        });
        scene.push(UiNode::Label {
            id: NodeId(2),
            bounds: Rect::new(1.0, 1.0, 2.0, 2.0),
            color: Color::TEXT,
            text: "top".into(),
        });
        let list = DisplayList::from_scene(&scene);
        assert_eq!(list.revision, 11);
        assert_eq!(list.clear_color, Color::rgba(8, 12, 18, 245));
        assert!(matches!(list.commands[0], DisplayCommand::Shape { .. }));
        assert!(matches!(list.commands[1], DisplayCommand::Text { .. }));
    }

    #[test]
    fn nested_transforms_and_clips_are_captured_per_command() {
        let mut builder = DisplayListBuilder::new(7, Color::BACKGROUND);
        builder.push_transform(Transform2D::translation(10.0, 5.0));
        builder.push_clip(Rect::new(0.0, 0.0, 20.0, 20.0));
        builder.push_clip(Rect::new(5.0, 4.0, 20.0, 8.0));
        builder.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::ACCENT);
        builder.pop_clip().unwrap();
        builder.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::TEXT);
        builder.pop_clip().unwrap();
        builder.pop_transform().unwrap();
        let list = builder.finish().unwrap();

        let DisplayCommand::Shape {
            transform, clip, ..
        } = &list.commands[0]
        else {
            unreachable!()
        };
        assert_eq!(*transform, Transform2D::translation(10.0, 5.0));
        assert_eq!(*clip, Some(Rect::new(15.0, 9.0, 15.0, 8.0)));
        let DisplayCommand::Shape { clip, .. } = &list.commands[1] else {
            unreachable!()
        };
        assert_eq!(*clip, Some(Rect::new(10.0, 5.0, 20.0, 20.0)));
    }

    #[test]
    fn disjoint_clip_suppresses_commands_and_unbalanced_scopes_are_errors() {
        let mut builder = DisplayListBuilder::new(0, Color::BACKGROUND);
        builder.push_clip(Rect::new(0.0, 0.0, 2.0, 2.0));
        builder.push_clip(Rect::new(4.0, 4.0, 2.0, 2.0));
        builder.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::ACCENT);
        assert!(builder.pop_clip().is_ok());
        assert!(builder.pop_clip().is_ok());
        assert!(builder.finish().unwrap().commands.is_empty());

        let mut unbalanced = DisplayListBuilder::new(0, Color::BACKGROUND);
        unbalanced.push_transform(Transform2D::scale(2.0, 2.0));
        assert_eq!(
            unbalanced.finish(),
            Err(DisplayListBuildError::UnbalancedTransforms)
        );
    }
}
