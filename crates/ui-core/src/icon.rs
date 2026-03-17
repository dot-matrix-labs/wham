use std::collections::HashMap;

use wham_core::types::Rect;

/// Identifier for a loaded icon within an icon pack.
/// Cheap to copy. Used by widgets to reference icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IconId(pub u16);

/// Metadata for a single icon in the atlas.
#[derive(Debug, Clone)]
pub struct IconEntry {
    pub id: IconId,
    pub name: String,
    /// Normalized UV rect in the icon atlas texture.
    pub uv: Rect,
    /// Native rasterized size in pixels.
    pub size_px: u16,
}

/// A loaded icon pack. Immutable after construction.
#[derive(Debug, Clone, Default)]
pub struct IconPack {
    pub name: String,
    entries: Vec<IconEntry>,
    by_name: HashMap<String, IconId>,
}

impl IconPack {
    /// Look up an icon by name, returning its `IconId`.
    pub fn get(&self, name: &str) -> Option<IconId> {
        self.by_name.get(name).copied()
    }

    /// Get the entry for a given `IconId`.
    ///
    /// # Panics
    /// Panics if `id` is out of range (programmer error).
    pub fn entry(&self, id: IconId) -> &IconEntry {
        &self.entries[id.0 as usize]
    }

    /// Returns `true` if the pack contains no icons.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of icons in the pack.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Build an `IconPack` from a JSON manifest string and the texture
    /// dimensions of the corresponding sprite sheet.
    ///
    /// The JSON format is:
    /// ```json
    /// {
    ///   "name": "my-icons",
    ///   "texture_size": [512, 512],
    ///   "icons": [
    ///     { "name": "check", "x": 0, "y": 0, "w": 24, "h": 24 },
    ///     ...
    ///   ]
    /// }
    /// ```
    pub fn from_manifest(json: &str) -> Result<Self, String> {
        let parsed: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;

        let pack_name = parsed
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed")
            .to_string();

        let texture_size = parsed
            .get("texture_size")
            .and_then(|v| v.as_array())
            .ok_or("missing texture_size")?;
        if texture_size.len() != 2 {
            return Err("texture_size must be [width, height]".to_string());
        }
        let tex_w = texture_size[0]
            .as_u64()
            .ok_or("texture_size[0] must be a number")? as f32;
        let tex_h = texture_size[1]
            .as_u64()
            .ok_or("texture_size[1] must be a number")? as f32;

        let icons_arr = parsed
            .get("icons")
            .and_then(|v| v.as_array())
            .ok_or("missing icons array")?;

        let mut entries = Vec::with_capacity(icons_arr.len());
        let mut by_name = HashMap::with_capacity(icons_arr.len());

        for (i, icon_val) in icons_arr.iter().enumerate() {
            let name = icon_val
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("icon[{i}] missing name"))?
                .to_string();
            let x = icon_val
                .get("x")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("icon[{i}] missing x"))? as u32;
            let y = icon_val
                .get("y")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("icon[{i}] missing y"))? as u32;
            let w = icon_val
                .get("w")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("icon[{i}] missing w"))? as u32;
            let h = icon_val
                .get("h")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| format!("icon[{i}] missing h"))? as u32;

            let id = IconId(i as u16);
            let uv = Rect::new(
                x as f32 / tex_w,
                y as f32 / tex_h,
                w as f32 / tex_w,
                h as f32 / tex_h,
            );
            let size_px = w.max(h) as u16;

            entries.push(IconEntry {
                id,
                name: name.clone(),
                uv,
                size_px,
            });
            by_name.insert(name, id);
        }

        Ok(Self {
            name: pack_name,
            entries,
            by_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_manifest_parses_icons() {
        let json = r#"{
            "name": "test-icons",
            "texture_size": [256, 256],
            "icons": [
                { "name": "check", "x": 0, "y": 0, "w": 24, "h": 24 },
                { "name": "close", "x": 24, "y": 0, "w": 24, "h": 24 }
            ]
        }"#;
        let pack = IconPack::from_manifest(json).unwrap();
        assert_eq!(pack.name, "test-icons");
        assert_eq!(pack.len(), 2);
        assert!(!pack.is_empty());

        let check_id = pack.get("check").unwrap();
        assert_eq!(check_id, IconId(0));
        let entry = pack.entry(check_id);
        assert_eq!(entry.name, "check");
        assert_eq!(entry.size_px, 24);
        // UV should be normalized: 24/256 = 0.09375
        assert!((entry.uv.x - 0.0).abs() < f32::EPSILON);
        assert!((entry.uv.w - 0.09375).abs() < f32::EPSILON);

        let close_id = pack.get("close").unwrap();
        assert_eq!(close_id, IconId(1));
        let close_entry = pack.entry(close_id);
        // x = 24/256 = 0.09375
        assert!((close_entry.uv.x - 0.09375).abs() < f32::EPSILON);
    }

    #[test]
    fn get_missing_icon_returns_none() {
        let pack = IconPack::default();
        assert!(pack.get("nonexistent").is_none());
    }

    #[test]
    fn from_manifest_rejects_invalid_json() {
        assert!(IconPack::from_manifest("not json").is_err());
    }

    #[test]
    fn from_manifest_rejects_missing_texture_size() {
        let json = r#"{ "name": "x", "icons": [] }"#;
        assert!(IconPack::from_manifest(json).is_err());
    }
}
