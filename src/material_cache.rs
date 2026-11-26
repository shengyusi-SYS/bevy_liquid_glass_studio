//! Intern immutable material configurations without retaining unused assets.
use std::collections::HashMap;

use bevy::{
    prelude::*,
    render::render_resource::{ShaderSize, encase},
};

use crate::material::{GlassMaterial, GlassUniform};

const UNIFORM_SIZE: usize = GlassUniform::SHADER_SIZE.get() as usize;

#[derive(PartialEq, Eq, Hash)]
struct MaterialKey {
    // Use the actual GPU representation so new uniform fields participate in
    // sharing automatically. Padding starts at zero and no heap buffer is needed.
    uniform: [u8; UNIFORM_SIZE],
    source: AssetId<Image>,
    blurred: AssetId<Image>,
}

#[derive(Resource, Default)]
pub(crate) struct SharedGlassMaterials(HashMap<MaterialKey, AssetId<GlassMaterial>>);

impl SharedGlassMaterials {
    pub fn resolve(
        &mut self,
        uniform: GlassUniform,
        source: &Handle<Image>,
        blurred: &Handle<Image>,
        assets: &mut Assets<GlassMaterial>,
    ) -> Handle<GlassMaterial> {
        let mut bytes = [0; UNIFORM_SIZE];
        encase::UniformBuffer::new(&mut bytes[..])
            .write(&uniform)
            .expect("GlassUniform has a fixed uniform-buffer layout");
        let key = MaterialKey {
            uniform: bytes,
            source: source.id(),
            blurred: blurred.id(),
        };
        if let Some(id) = self.0.get(&key)
            && let Some(handle) = assets.get_strong_handle(*id)
        {
            return handle;
        }
        let handle = assets.add(GlassMaterial {
            uniform,
            source_image: source.clone(),
            blurred_image: blurred.clone(),
        });
        self.0.insert(key, handle.id());
        handle
    }

    pub fn retain_live(&mut self, assets: &Assets<GlassMaterial>) {
        self.0.retain(|_, id| assets.contains(*id));
    }

    pub fn refresh_images(
        &self,
        changed: &std::collections::HashSet<AssetId<Image>>,
        assets: &mut Assets<GlassMaterial>,
    ) {
        if changed.is_empty() {
            return;
        }
        for (key, id) in &self.0 {
            if (changed.contains(&key.source) || changed.contains(&key.blurred))
                && let Some(material) = assets.get_mut(*id)
            {
                // The Image may have a new GPU view under the same AssetId.
                material.into_inner();
            }
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}
