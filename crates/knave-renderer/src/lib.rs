//! wgpu-backed rendering boundary for Knave UI scenes.

mod painter;
pub use painter::WgpuPainter;

use knave_ui::UiScene;
pub use knave_ui::{DisplayCommand, DisplayList, RenderCommand, RenderList};

pub struct WgpuRenderer {
    instance: wgpu::Instance,
}

impl WgpuRenderer {
    pub fn new() -> Self {
        Self {
            instance: wgpu::Instance::default(),
        }
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn prepare(&self, scene: &UiScene) -> RenderList {
        RenderList::from_scene(scene)
    }
}

impl Default for WgpuRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_conversion_preserves_revision_and_order() {
        let scene = UiScene::bar(4, 1280.0, 36.0);
        let renderer = WgpuRenderer::new();
        let list = renderer.prepare(&scene);

        assert_eq!(list.revision, 4);
        assert_eq!(list.commands.len(), 2);
        assert!(matches!(list.commands[0], DisplayCommand::FillRect { .. }));
        assert!(matches!(list.commands[1], DisplayCommand::Text { .. }));
    }
}
