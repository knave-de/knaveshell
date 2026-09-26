use std::{error::Error, fmt};

use knave_ui::{DisplayCommand, Rect};

use crate::{
    images::ImagePass,
    shapes::ShapePass,
    text::{TextItem, TextPass, TextPassError},
};

const MAX_DISPLAY_COMMANDS_PER_FRAME: usize = 8192;

#[derive(Debug)]
pub enum PainterError {
    PrepareText(glyphon::PrepareError),
    RenderText(glyphon::RenderError),
    TooManyCommands(usize),
    TooManyTextRuns,
    UnsupportedTextTransform,
    TextBytesExceeded,
    FontFamilyTooLong,
}

impl fmt::Display for PainterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrepareText(error) => write!(formatter, "text preparation failed: {error}"),
            Self::RenderText(error) => write!(formatter, "text rendering failed: {error}"),
            Self::TooManyCommands(count) => write!(
                formatter,
                "display list has {count} commands; limit is {MAX_DISPLAY_COMMANDS_PER_FRAME}"
            ),
            Self::TooManyTextRuns => write!(
                formatter,
                "display list exceeds the per-frame text run limit"
            ),
            Self::UnsupportedTextTransform => write!(
                formatter,
                "text currently supports translation and positive uniform scale only"
            ),
            Self::TextBytesExceeded => {
                write!(
                    formatter,
                    "display list exceeds the per-frame text byte limit"
                )
            }
            Self::FontFamilyTooLong => write!(formatter, "font family name exceeds 256 bytes"),
        }
    }
}

impl Error for PainterError {}

pub struct WgpuPainter {
    shapes: ShapePass,
    images: ImagePass,
    text: TextPass,
}

impl WgpuPainter {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            shapes: ShapePass::new(device, format),
            images: ImagePass::new(device, format),
            text: TextPass::new(device, queue, format),
        }
    }

    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        viewport: (u32, u32),
        render_list: &knave_ui::DisplayList,
    ) -> Result<(), PainterError> {
        if render_list.commands.len() > MAX_DISPLAY_COMMANDS_PER_FRAME {
            return Err(PainterError::TooManyCommands(render_list.commands.len()));
        }
        self.shapes
            .prepare(device, queue, viewport, &render_list.commands);
        self.images
            .prepare(device, queue, viewport, &render_list.commands);
        self.text.begin_frame(queue, viewport);

        let mut draws = Vec::with_capacity(render_list.commands.len());
        let mut command_index = 0;
        while command_index < render_list.commands.len() {
            match &render_list.commands[command_index] {
                DisplayCommand::Shape { .. } => {
                    if let Some((instance_index, clip)) =
                        self.shapes.draw_for_command(command_index)
                        && let Some(scissor) = scissor_rect(clip, viewport)
                    {
                        draws.push(PreparedDraw::Shape {
                            instance_index,
                            scissor,
                        });
                    }
                    command_index += 1;
                }
                DisplayCommand::Image { .. } => {
                    if let Some(image) = self.images.draws[command_index]
                        && let Some(scissor) = scissor_rect(image.clip, viewport)
                    {
                        draws.push(PreparedDraw::Image {
                            instance_index: image.instance_index,
                            scissor,
                        });
                    }
                    command_index += 1;
                }
                DisplayCommand::Text { .. } => {
                    let mut items = Vec::new();
                    while let Some(DisplayCommand::Text {
                        bounds,
                        style,
                        text,
                        transform,
                        clip,
                    }) = render_list.commands.get(command_index)
                    {
                        items.push(TextItem {
                            bounds: *bounds,
                            text,
                            style,
                            transform: *transform,
                            clip: *clip,
                        });
                        command_index += 1;
                    }
                    let run = self
                        .text
                        .prepare_run(device, queue, &items)
                        .map_err(|error| match error {
                            TextPassError::Glyphon(error) => PainterError::PrepareText(error),
                            TextPassError::TooManyRuns => PainterError::TooManyTextRuns,
                            TextPassError::UnsupportedTransform => {
                                PainterError::UnsupportedTextTransform
                            }
                            TextPassError::TextBytesExceeded => PainterError::TextBytesExceeded,
                            TextPassError::FontFamilyTooLong => PainterError::FontFamilyTooLong,
                        })?;
                    if run != usize::MAX {
                        draws.push(PreparedDraw::Text { run });
                    }
                }
            }
        }

        if draws.is_empty() {
            return Ok(());
        }
        let full_scissor = [0, 0, viewport.0.max(1), viewport.1.max(1)];
        let Self {
            shapes,
            images,
            text,
        } = self;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("knave-renderer-ordered-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        for draw in draws {
            match draw {
                PreparedDraw::Shape {
                    instance_index,
                    scissor,
                } => shapes.draw(&mut pass, instance_index, scissor),
                PreparedDraw::Image {
                    instance_index,
                    scissor,
                } => images.draw(&mut pass, instance_index, scissor),
                PreparedDraw::Text { run } => {
                    pass.set_scissor_rect(
                        full_scissor[0],
                        full_scissor[1],
                        full_scissor[2],
                        full_scissor[3],
                    );
                    text.render(run, &mut pass)
                        .map_err(PainterError::RenderText)?;
                }
            }
        }
        Ok(())
    }
}

enum PreparedDraw {
    Shape {
        instance_index: usize,
        scissor: [u32; 4],
    },
    Image {
        instance_index: usize,
        scissor: [u32; 4],
    },
    Text {
        run: usize,
    },
}

fn scissor_rect(clip: Option<Rect>, viewport: (u32, u32)) -> Option<[u32; 4]> {
    let viewport_rect = Rect::new(0.0, 0.0, viewport.0 as f32, viewport.1 as f32);
    let visible = match clip {
        Some(clip) => clip.intersection(viewport_rect)?,
        None if viewport.0 > 0 && viewport.1 > 0 => viewport_rect,
        None => return None,
    };
    let left = visible.x.floor().max(0.0) as u32;
    let top = visible.y.floor().max(0.0) as u32;
    let right = (visible.x + visible.width).ceil().min(viewport.0 as f32) as u32;
    let bottom = (visible.y + visible.height).ceil().min(viewport.1 as f32) as u32;
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scissor_clips_to_target_and_rejects_empty_bounds() {
        assert_eq!(
            scissor_rect(Some(Rect::new(-2.0, 4.0, 8.5, 12.0)), (10, 10)),
            Some([0, 4, 7, 6])
        );
        assert_eq!(
            scissor_rect(Some(Rect::new(12.0, 0.0, 2.0, 2.0)), (10, 10)),
            None
        );
    }

    #[test]
    fn display_command_order_is_not_partitioned_by_primitive_type() {
        let list = knave_ui::DisplayList {
            revision: 0,
            clear_color: knave_ui::Color::BACKGROUND,
            commands: vec![
                DisplayCommand::Shape {
                    bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
                    paint: knave_ui::ShapePaint::fill(knave_ui::Color::ACCENT),
                    transform: knave_ui::Transform2D::IDENTITY,
                    clip: None,
                },
                DisplayCommand::Image {
                    bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
                    image: knave_ui::UiImage::from_rgba(1, 1, vec![255; 4]).unwrap(),
                    style: knave_ui::ImageStyle::default(),
                    transform: knave_ui::Transform2D::IDENTITY,
                    clip: None,
                },
                DisplayCommand::Shape {
                    bounds: Rect::new(0.0, 0.0, 4.0, 4.0),
                    paint: knave_ui::ShapePaint::fill(knave_ui::Color::TEXT),
                    transform: knave_ui::Transform2D::IDENTITY,
                    clip: None,
                },
            ],
        };
        let kinds = list
            .commands
            .iter()
            .map(|command| match command {
                DisplayCommand::Shape { .. } => 0,
                DisplayCommand::Text { .. } => 1,
                DisplayCommand::Image { .. } => 2,
            })
            .collect::<Vec<_>>();
        assert_eq!(kinds, [0, 2, 0]);
    }
}
