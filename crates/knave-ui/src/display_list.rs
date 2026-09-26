use crate::{Color, Rect, UiImage, UiNode, UiScene};

/// A renderer-independent instruction in front-to-back paint order.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommand {
    FillRect {
        bounds: Rect,
        color: Color,
    },
    Text {
        bounds: Rect,
        color: Color,
        text: String,
    },
    Image {
        bounds: Rect,
        image: UiImage,
    },
}

/// The ordered paint description produced by a UI scene.
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
        let commands = scene
            .nodes()
            .iter()
            .map(|node| match node {
                UiNode::Panel { bounds, color, .. } => DisplayCommand::FillRect {
                    bounds: *bounds,
                    color: *color,
                },
                UiNode::Label {
                    bounds,
                    color,
                    text,
                    ..
                } => DisplayCommand::Text {
                    bounds: *bounds,
                    color: *color,
                    text: text.clone(),
                },
                UiNode::Image { bounds, image, .. } => DisplayCommand::Image {
                    bounds: *bounds,
                    image: image.clone(),
                },
            })
            .collect();
        Self {
            revision: scene.revision(),
            clear_color: scene.clear_color(),
            commands,
        }
    }
}

impl Default for DisplayList {
    fn default() -> Self {
        Self::new(0, Color::BACKGROUND)
    }
}

pub type RenderCommand = DisplayCommand;
pub type RenderList = DisplayList;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_remain_in_insertion_order() {
        let mut list = DisplayList::new(3, Color::BACKGROUND);
        list.commands.push(DisplayCommand::Image {
            bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
            image: UiImage::from_rgba(1, 1, vec![255; 4]).unwrap(),
        });
        list.commands.push(DisplayCommand::FillRect {
            bounds: Rect::new(0.0, 0.0, 2.0, 2.0),
            color: Color::ACCENT,
        });
        assert!(matches!(list.commands[0], DisplayCommand::Image { .. }));
        assert!(matches!(list.commands[1], DisplayCommand::FillRect { .. }));
    }

    #[test]
    fn scene_projection_preserves_order_revision_and_clear_color() {
        let mut scene = UiScene::new(11);
        scene.set_clear_color(Color::rgba(8, 12, 18, 245));
        scene.push(UiNode::Panel {
            id: crate::NodeId(1),
            bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
            color: Color::ACCENT,
        });
        scene.push(UiNode::Label {
            id: crate::NodeId(2),
            bounds: Rect::new(1.0, 1.0, 2.0, 2.0),
            color: Color::TEXT,
            text: "top".into(),
        });
        let list = DisplayList::from_scene(&scene);
        assert_eq!(list.revision, 11);
        assert_eq!(list.clear_color, Color::rgba(8, 12, 18, 245));
        assert!(matches!(list.commands[0], DisplayCommand::FillRect { .. }));
        assert!(matches!(list.commands[1], DisplayCommand::Text { .. }));
    }
}
