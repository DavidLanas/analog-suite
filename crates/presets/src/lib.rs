//! Formato de presets serializable en JSON, con versión.
//!
//! * [`Preset`]: un plugin, sus parámetros normalizados (0..1) indexados por id de parámetro.
//! * [`ChainPreset`]: varios presets a la vez (p. ej. "Funk Chain": FunkComp + PultecMaster +
//!   TapeMaster).
//!
//! Los valores se guardan **normalizados** para que el formato no dependa de rangos ni
//! unidades que puedan cambiar entre versiones del plugin. La versión del formato permite
//! migrar archivos antiguos y rechazar los de una versión futura.

pub mod store;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Versión actual del formato.
pub const FORMAT_VERSION: u32 = 1;

/// Categoría de los presets que guarda el usuario.
pub const CATEGORY_USER: &str = "User";

#[derive(Debug, PartialEq)]
pub enum PresetError {
    Json(String),
    /// El archivo lo escribió una versión más nueva que esta.
    TooNew {
        found: u32,
        supported: u32,
    },
}

impl std::fmt::Display for PresetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PresetError::Json(e) => write!(f, "JSON inválido: {e}"),
            PresetError::TooNew { found, supported } => write!(
                f,
                "preset de formato v{found}; esta versión sólo entiende hasta v{supported}"
            ),
        }
    }
}

impl std::error::Error for PresetError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    pub version: u32,
    /// Identificador del plugin ("funkcomp", "pultecmaster", "tapemaster").
    pub plugin: String,
    pub name: String,
    /// Categoría para agrupar en la lista ("Voice", "Instruments", "Mastering", "User").
    /// Opcional al cargar: los archivos sin categoría se tratan como del usuario.
    #[serde(default)]
    pub category: String,
    /// Texto libre; en los presets de carácter indicar "inspirado en …".
    #[serde(default)]
    pub description: String,
    /// id de parámetro -> valor normalizado (0..1).
    pub params: BTreeMap<String, f32>,
}

impl Preset {
    pub fn new(plugin: &str, name: &str) -> Self {
        Self {
            version: FORMAT_VERSION,
            plugin: plugin.to_owned(),
            name: name.to_owned(),
            category: String::new(),
            description: String::new(),
            params: BTreeMap::new(),
        }
    }

    pub fn with_category(mut self, category: &str) -> Self {
        self.category = category.to_owned();
        self
    }

    /// Categoría efectiva: la vacía cuenta como [`CATEGORY_USER`].
    pub fn category_or_user(&self) -> &str {
        if self.category.is_empty() {
            CATEGORY_USER
        } else {
            &self.category
        }
    }

    /// `true` si todos los parámetros de este preset están en `other` con diferencia <= `tol`.
    pub fn same_params(&self, other: &Preset, tol: f32) -> bool {
        store::params_match(&self.params, &other.params, tol)
    }

    pub fn with_param(mut self, id: &str, normalized: f32) -> Self {
        self.params
            .insert(id.to_owned(), normalized.clamp(0.0, 1.0));
        self
    }

    pub fn with_description(mut self, text: &str) -> Self {
        self.description = text.to_owned();
        self
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Preset siempre es serializable")
    }

    pub fn from_json(json: &str) -> Result<Self, PresetError> {
        let mut p: Preset =
            serde_json::from_str(json).map_err(|e| PresetError::Json(e.to_string()))?;
        check_version(p.version)?;
        migrate_preset(&mut p);
        Ok(p)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChainPreset {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub members: Vec<Preset>,
}

impl ChainPreset {
    pub fn new(name: &str) -> Self {
        Self {
            version: FORMAT_VERSION,
            name: name.to_owned(),
            description: String::new(),
            members: Vec::new(),
        }
    }

    pub fn with_member(mut self, preset: Preset) -> Self {
        self.members.push(preset);
        self
    }

    /// Parte de la cadena que corresponde a un plugin concreto.
    pub fn member_for(&self, plugin: &str) -> Option<&Preset> {
        self.members.iter().find(|m| m.plugin == plugin)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("ChainPreset siempre es serializable")
    }

    pub fn from_json(json: &str) -> Result<Self, PresetError> {
        let mut c: ChainPreset =
            serde_json::from_str(json).map_err(|e| PresetError::Json(e.to_string()))?;
        check_version(c.version)?;
        for m in &mut c.members {
            check_version(m.version)?;
            migrate_preset(m);
        }
        Ok(c)
    }
}

fn check_version(found: u32) -> Result<(), PresetError> {
    if found > FORMAT_VERSION {
        Err(PresetError::TooNew {
            found,
            supported: FORMAT_VERSION,
        })
    } else {
        Ok(())
    }
}

/// Punto de extensión: cuando `FORMAT_VERSION` suba, convertir aquí los presets antiguos.
fn migrate_preset(p: &mut Preset) {
    p.version = FORMAT_VERSION;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_roundtrip() {
        let p = Preset::new("funkcomp", "Funk Shout")
            .with_description("inspirado en un compresor FET clásico")
            .with_param("mode", 0.0)
            .with_param("mix", 0.6);
        let back = Preset::from_json(&p.to_json()).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn params_are_clamped() {
        let p = Preset::new("x", "y")
            .with_param("a", 1.7)
            .with_param("b", -3.0);
        assert_eq!(p.params["a"], 1.0);
        assert_eq!(p.params["b"], 0.0);
    }

    #[test]
    fn chain_lookup_and_roundtrip() {
        let chain = ChainPreset::new("Funk Chain")
            .with_member(Preset::new("funkcomp", "a").with_param("mix", 1.0))
            .with_member(Preset::new("tapemaster", "b"));
        let back = ChainPreset::from_json(&chain.to_json()).unwrap();
        assert_eq!(chain, back);
        assert!(back.member_for("tapemaster").is_some());
        assert!(back.member_for("pultecmaster").is_none());
    }

    #[test]
    fn rejects_future_versions_and_garbage() {
        let future = r#"{"version": 99, "plugin": "x", "name": "n", "params": {}}"#;
        assert_eq!(
            Preset::from_json(future),
            Err(PresetError::TooNew {
                found: 99,
                supported: FORMAT_VERSION
            })
        );
        assert!(matches!(
            Preset::from_json("no es json"),
            Err(PresetError::Json(_))
        ));
    }

    #[test]
    fn description_is_optional_on_load() {
        let json = r#"{"version": 1, "plugin": "x", "name": "n", "params": {"g": 0.5}}"#;
        let p = Preset::from_json(json).unwrap();
        assert_eq!(p.description, "");
        assert_eq!(p.params["g"], 0.5);
    }

    #[test]
    fn category_roundtrips_and_defaults_to_user_for_old_files() {
        let p = Preset::new("funkcomp", "x").with_category("Voice");
        let back = Preset::from_json(&p.to_json()).unwrap();
        assert_eq!(back.category, "Voice");
        assert_eq!(back.category_or_user(), "Voice");
        // Un archivo anterior a este campo sigue cargando y cuenta como del usuario.
        let old = r#"{"version": 1, "plugin": "x", "name": "n", "params": {}}"#;
        let p = Preset::from_json(old).unwrap();
        assert_eq!(p.category, "");
        assert_eq!(p.category_or_user(), CATEGORY_USER);
    }

    #[test]
    fn same_params_detects_modifications() {
        let base = Preset::new("x", "a")
            .with_param("g", 0.5)
            .with_param("h", 0.1);
        let same = Preset::new("x", "b")
            .with_param("g", 0.5004)
            .with_param("h", 0.1)
            .with_param("z", 1.0);
        let changed = Preset::new("x", "c")
            .with_param("g", 0.6)
            .with_param("h", 0.1);
        assert!(base.same_params(&same, 2e-3));
        assert!(!base.same_params(&changed, 2e-3));
    }
}
