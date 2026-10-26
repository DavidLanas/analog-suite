//! Navegador de presets de FunkComp: presets de fábrica + presets de usuario en disco.
//!
//! Vive en el estado de la GUI (`T` de `create_egui_editor`), así que sólo lo toca el hilo de
//! la interfaz: el hilo de audio nunca lee ni escribe presets ni archivos.
//!
//! "Modificado": al cargar un preset se espera unos fotogramas a que el host aplique los
//! valores y se toma entonces una instantánea como línea base; a partir de ahí cualquier
//! diferencia con esa base marca el preset como modificado (así no importa cómo cuantiza cada
//! parámetro). Al abrir el editor se intenta reconocer el preset vigente comparando con la
//! lista (con tolerancia).

use crate::params::{FunkCompParams, PLUGIN_ID};
use funkcomp_dsp::factory;
use nih_plug::prelude::ParamSetter;
use presets::{store, Preset, CATEGORY_USER};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Tolerancia para reconocer un preset al abrir el editor (1 % del recorrido).
const MATCH_TOL: f32 = 0.01;
/// Tolerancia para decir que algo cambió desde la línea base.
const MODIFIED_TOL: f32 = 1e-4;
/// Fotogramas de espera tras cargar un preset antes de tomar la línea base.
const SETTLE_FRAMES: u8 = 4;

/// Categorías con las que se puede guardar un preset de usuario (nombre guardado, etiqueta).
pub const SAVE_CATEGORIES: [(&str, &str); 4] = [
    ("Voice", "Voz"),
    ("Instruments", "Instrumentos"),
    ("Mastering", "Mastering"),
    (CATEGORY_USER, "Otros"),
];

/// Filtros de la lista (valor, etiqueta). `None` = todos.
pub const FILTERS: [(Option<&str>, &str); 5] = [
    (None, "Todos"),
    (Some("Voice"), "Voz"),
    (Some("Instruments"), "Instrumentos"),
    (Some("Mastering"), "Mastering"),
    (Some(CATEGORY_USER), "Otros"),
];

/// Etiqueta en español de una categoría guardada.
pub fn category_label(category: &str) -> &str {
    SAVE_CATEGORIES
        .iter()
        .find(|(c, _)| *c == category)
        .map_or(category, |(_, l)| l)
}

/// De dónde viene un preset.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Factory(usize),
    User(PathBuf),
}

pub struct Browser {
    factory: Vec<Preset>,
    user: Vec<(PathBuf, Preset)>,
    dir: Option<PathBuf>,
    pub filter: Option<&'static str>,
    current: Option<Source>,
    baseline: Option<BTreeMap<String, f32>>,
    settle: u8,
    pub saving: bool,
    pub focus_name: bool,
    pub save_name: String,
    pub save_category: &'static str,
    pub confirm_delete: bool,
    pub message: String,
}

impl Browser {
    pub fn new(params: &FunkCompParams) -> Self {
        let factory = factory::presets()
            .iter()
            .map(|f| {
                params.preset_from_settings(f.name, f.category.as_str(), f.description, &f.settings)
            })
            .collect();
        let mut b = Self {
            factory,
            user: Vec::new(),
            dir: store::user_dir(PLUGIN_ID),
            filter: None,
            current: None,
            baseline: None,
            settle: 0,
            saving: false,
            focus_name: false,
            save_name: String::new(),
            save_category: CATEGORY_USER,
            confirm_delete: false,
            message: String::new(),
        };
        b.refresh_user();
        b.identify(&params.snapshot());
        b
    }

    /// Vuelve a leer la carpeta de presets de usuario. Los archivos que no se pueden leer o no
    /// son de FunkComp se ignoran (se avisa cuántos).
    pub fn refresh_user(&mut self) {
        self.user.clear();
        let Some(dir) = &self.dir else { return };
        let mut skipped = 0;
        for path in store::list_files(dir) {
            match store::read(&path)
                .ok()
                .and_then(|json| Preset::from_json(&json).ok())
            {
                Some(p) if p.plugin == PLUGIN_ID => self.user.push((path, p)),
                _ => skipped += 1,
            }
        }
        if skipped > 0 {
            self.message = format!("{skipped} archivo(s) de presets ignorados (no válidos)");
        }
    }

    fn matches_filter(&self, p: &Preset) -> bool {
        match self.filter {
            None => true,
            Some(f) => p.category_or_user() == f,
        }
    }

    /// Entradas visibles con el filtro actual: primero las de fábrica, luego las del usuario.
    pub fn entries(&self) -> Vec<Source> {
        let factory = self
            .factory
            .iter()
            .enumerate()
            .filter(|(_, p)| self.matches_filter(p))
            .map(|(i, _)| Source::Factory(i));
        let user = self
            .user
            .iter()
            .filter(|(_, p)| self.matches_filter(p))
            .map(|(path, _)| Source::User(path.clone()));
        factory.chain(user).collect()
    }

    pub fn preset(&self, source: &Source) -> Option<&Preset> {
        match source {
            Source::Factory(i) => self.factory.get(*i),
            Source::User(path) => self.user.iter().find(|(p, _)| p == path).map(|(_, p)| p),
        }
    }

    pub fn current(&self) -> Option<&Source> {
        self.current.as_ref()
    }

    pub fn current_preset(&self) -> Option<&Preset> {
        self.current.as_ref().and_then(|s| self.preset(s))
    }

    pub fn current_is_user(&self) -> bool {
        matches!(self.current, Some(Source::User(_)))
    }

    /// Cuenta fotogramas tras cargar un preset y toma la línea base cuando se asentó.
    /// Llamar una vez por fotograma de la GUI.
    pub fn tick(&mut self, snapshot: &BTreeMap<String, f32>) {
        if self.settle > 0 {
            self.settle -= 1;
            if self.settle == 0 {
                self.baseline = Some(snapshot.clone());
            }
        }
    }

    /// `true` si los parámetros difieren de lo que se cargó/guardó.
    pub fn is_modified(&self, snapshot: &BTreeMap<String, f32>) -> bool {
        match &self.baseline {
            Some(base) => {
                !store::params_match(base, snapshot, MODIFIED_TOL)
                    || !store::params_match(snapshot, base, MODIFIED_TOL)
            }
            None => false,
        }
    }

    /// Texto del botón principal: nombre del preset (con `*` si está modificado).
    pub fn title(&self, snapshot: &BTreeMap<String, f32>) -> String {
        match self.current_preset() {
            Some(p) if self.is_modified(snapshot) => format!("{} *", p.name),
            Some(p) => p.name.clone(),
            None => "Sin preset".to_owned(),
        }
    }

    /// Reconoce el preset vigente comparando con la lista (tolerancia [`MATCH_TOL`]).
    pub fn identify(&mut self, snapshot: &BTreeMap<String, f32>) {
        let probe = Preset {
            params: snapshot.clone(),
            ..Preset::new(PLUGIN_ID, "")
        };
        let found = self
            .factory
            .iter()
            .position(|p| p.same_params(&probe, MATCH_TOL))
            .map(Source::Factory)
            .or_else(|| {
                self.user
                    .iter()
                    .find(|(_, p)| p.same_params(&probe, MATCH_TOL))
                    .map(|(path, _)| Source::User(path.clone()))
            });
        if found.is_some() {
            self.baseline = Some(snapshot.clone());
        }
        self.current = found;
    }

    /// Entrada vecina a la actual (con el filtro actual), dando la vuelta en los extremos.
    /// Sin preset vigente, `+1` va a la primera y `-1` a la última.
    pub fn neighbor(&self, step: i32) -> Option<Source> {
        let entries = self.entries();
        if entries.is_empty() {
            return None;
        }
        let n = entries.len() as i32;
        let pos = self
            .current
            .as_ref()
            .and_then(|c| entries.iter().position(|e| e == c));
        let next = match pos {
            Some(i) => (i as i32 + step).rem_euclid(n),
            None if step >= 0 => 0,
            None => n - 1,
        };
        entries.into_iter().nth(next as usize)
    }

    pub fn step(&mut self, step: i32, setter: &ParamSetter, params: &FunkCompParams) {
        if let Some(src) = self.neighbor(step) {
            self.load(src, setter, params);
        }
    }

    pub fn load(&mut self, source: Source, setter: &ParamSetter, params: &FunkCompParams) {
        let Some(preset) = self.preset(&source).cloned() else {
            return;
        };
        params.apply_preset(setter, &preset);
        self.current = Some(source);
        self.baseline = None;
        self.settle = SETTLE_FRAMES;
        self.confirm_delete = false;
        self.saving = false;
        self.message.clear();
    }

    /// "Guardar": sobrescribe el preset de usuario vigente; si no lo hay (o es de fábrica)
    /// abre el diálogo "Guardar como".
    pub fn save(&mut self, params: &FunkCompParams) {
        let (Some(Source::User(path)), Some(old)) = (self.current.clone(), self.current_preset())
        else {
            self.begin_save_as();
            return;
        };
        let mut preset = old.clone();
        preset.params = params.snapshot();
        match store::write_atomic(&path, &preset.to_json()) {
            Ok(()) => {
                self.baseline = Some(preset.params.clone());
                self.message = format!("Guardado: {}", preset.name);
                self.refresh_user();
            }
            Err(e) => self.message = format!("No se pudo guardar: {e}"),
        }
    }

    pub fn begin_save_as(&mut self) {
        self.saving = true;
        self.focus_name = true;
        self.confirm_delete = false;
        self.save_name = match self.current_preset() {
            Some(p) if self.current_is_user() => p.name.clone(),
            Some(p) => format!("{} (mío)", p.name),
            None => "Mi preset".to_owned(),
        };
        self.save_category = match self.current_preset() {
            Some(p) => SAVE_CATEGORIES
                .iter()
                .map(|(c, _)| *c)
                .find(|c| *c == p.category_or_user())
                .unwrap_or(CATEGORY_USER),
            None => CATEGORY_USER,
        };
    }

    /// `true` si guardar con el nombre actual pisaría un archivo existente.
    pub fn save_would_overwrite(&self) -> bool {
        self.dir
            .as_ref()
            .is_some_and(|d| store::file_path(d, self.save_name.trim()).exists())
    }

    pub fn save_as(&mut self, params: &FunkCompParams) {
        let name = self.save_name.trim().to_owned();
        if name.is_empty() {
            self.message = "Escribe un nombre para el preset".to_owned();
            return;
        }
        let Some(dir) = self.dir.clone() else {
            self.message = "No se encontró la carpeta de configuración (HOME)".to_owned();
            return;
        };
        let mut preset = Preset::new(PLUGIN_ID, &name).with_category(self.save_category);
        preset.params = params.snapshot();
        match store::save(&dir, &name, &preset.to_json()) {
            Ok(path) => {
                self.refresh_user();
                self.current = Some(Source::User(path));
                self.baseline = Some(preset.params);
                self.saving = false;
                self.message = format!("Guardado: {name}");
            }
            Err(e) => self.message = format!("No se pudo guardar: {e}"),
        }
    }

    /// Borra el preset de usuario vigente (el de fábrica no se puede borrar).
    pub fn delete_current(&mut self) {
        self.confirm_delete = false;
        let (Some(Source::User(path)), Some(dir)) = (self.current.clone(), self.dir.clone()) else {
            return;
        };
        match store::delete(&dir, &path) {
            Ok(()) => {
                self.current = None;
                self.baseline = None;
                self.refresh_user();
                self.message = "Preset borrado".to_owned();
            }
            Err(e) => self.message = format!("No se pudo borrar: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser() -> (FunkCompParams, Browser) {
        let params = FunkCompParams::default();
        let mut b = Browser::new(&params);
        // Aislar los tests de los presets reales del usuario.
        b.user.clear();
        b.dir = None;
        b.current = None;
        (params, b)
    }

    #[test]
    fn factory_presets_are_complete_and_categorised() {
        let (params, b) = browser();
        assert_eq!(b.factory.len(), factory::presets().len());
        let keys = params.snapshot().len();
        for p in &b.factory {
            assert_eq!(p.params.len(), keys, "{} incompleto", p.name);
            assert!(
                SAVE_CATEGORIES.iter().any(|(c, _)| *c == p.category),
                "{}: categoría {:?}",
                p.name,
                p.category
            );
            assert!(p.params.values().all(|v| (0.0..=1.0).contains(v)));
        }
    }

    #[test]
    fn filters_split_the_list_by_category() {
        let (_, mut b) = browser();
        let all = b.entries().len();
        let mut sum = 0;
        for (filter, _) in FILTERS.iter().skip(1) {
            b.filter = *filter;
            let n = b.entries().len();
            for src in b.entries() {
                assert_eq!(Some(b.preset(&src).unwrap().category_or_user()), *filter,);
            }
            sum += n;
        }
        assert_eq!(sum, all, "toda entrada cae en exactamente un filtro");
        b.filter = Some("Voice");
        assert!(b.entries().len() >= 5 && b.entries().len() < all);
    }

    #[test]
    fn identify_recognises_a_factory_preset_by_its_values() {
        let (_, mut b) = browser();
        let snapshot = b.factory[3].params.clone();
        b.identify(&snapshot);
        assert_eq!(b.current(), Some(&Source::Factory(3)));
        assert!(!b.is_modified(&snapshot));

        let mut tweaked = snapshot.clone();
        let mix = tweaked["mix"];
        *tweaked.get_mut("mix").unwrap() = if mix > 0.5 { mix - 0.3 } else { mix + 0.3 };
        assert!(b.is_modified(&tweaked));
        assert!(b.title(&tweaked).ends_with(" *"));
        assert!(!b.title(&snapshot).ends_with('*'));
    }

    #[test]
    fn identify_finds_nothing_for_unrelated_values() {
        let (params, mut b) = browser();
        let mut weird = params.snapshot();
        *weird.get_mut("input").unwrap() = 0.123;
        *weird.get_mut("threshold").unwrap() = 0.987;
        b.identify(&weird);
        assert_eq!(b.current(), None);
        assert!(
            !b.is_modified(&weird),
            "sin preset no hay nada que modificar"
        );
        assert_eq!(b.title(&weird), "Sin preset");
    }

    #[test]
    fn neighbor_wraps_around_and_respects_the_filter() {
        let (_, mut b) = browser();
        let n = b.entries().len();
        assert_eq!(b.neighbor(1), Some(Source::Factory(0)));
        assert_eq!(b.neighbor(-1), b.entries().pop());

        b.current = Some(Source::Factory(n - 1));
        assert_eq!(b.neighbor(1), Some(Source::Factory(0)), "da la vuelta");
        b.current = Some(Source::Factory(0));
        assert_eq!(b.neighbor(-1), Some(Source::Factory(n - 1)));
        assert_eq!(b.neighbor(1), Some(Source::Factory(1)));

        b.filter = Some("Mastering");
        let list = b.entries();
        assert!(!list.is_empty());
        b.current = list.last().cloned();
        assert_eq!(b.neighbor(1), list.first().cloned());
    }

    #[test]
    fn baseline_is_taken_after_the_settle_frames() {
        let (params, mut b) = browser();
        let snap = params.snapshot();
        b.settle = SETTLE_FRAMES;
        b.current = Some(Source::Factory(0));
        for _ in 0..SETTLE_FRAMES - 1 {
            b.tick(&snap);
            assert!(b.baseline.is_none());
        }
        b.tick(&snap);
        assert_eq!(b.baseline.as_ref(), Some(&snap));
    }

    #[test]
    fn save_without_folder_reports_an_error_instead_of_panicking() {
        let (params, mut b) = browser();
        b.save_name = "Algo".to_owned();
        b.save_as(&params);
        assert!(b.message.contains("carpeta"), "{}", b.message);
        b.save_name = "  ".to_owned();
        b.save_as(&params);
        assert!(b.message.contains("nombre"));
    }
}
