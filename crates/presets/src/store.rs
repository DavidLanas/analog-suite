//! Almacenamiento de presets de usuario en disco (sólo `std`, sin dependencias).
//!
//! Los presets de usuario viven como archivos `.json` en
//! `$XDG_CONFIG_HOME/analog-suite/<plugin>/presets/` (o `~/.config/analog-suite/...`).
//! Todo esto debe llamarse desde la GUI o desde `initialize`, **nunca** desde el hilo de audio.
//!
//! Seguridad: el nombre que escribe el usuario nunca se usa como ruta; se reduce a un nombre
//! de archivo seguro con [`sanitize_file_stem`] (sin separadores, sin `..`, sin archivos
//! ocultos), así que no se puede escribir fuera de la carpeta de presets.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// Máximo de caracteres del nombre de archivo (sin extensión).
const MAX_STEM_CHARS: usize = 64;

/// Raíz de configuración a partir de las variables de entorno (inyectables para los tests).
pub fn config_root_from(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg_config_home.filter(|s| !s.is_empty()) {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(home.filter(|s| !s.is_empty())?).join(".config"),
    };
    Some(base.join("analog-suite"))
}

/// Carpeta de presets de usuario de un plugin (no se crea hasta guardar).
pub fn user_dir(plugin: &str) -> Option<PathBuf> {
    let root = config_root_from(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )?;
    Some(root.join(sanitize_file_stem(plugin)).join("presets"))
}

/// Reduce un nombre libre a un nombre de archivo seguro: conserva letras y dígitos (también
/// con acentos), espacios, `-` y `_`; todo lo demás pasa a `_`. Recorta espacios y largo; nunca
/// devuelve una cadena vacía.
pub fn sanitize_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(MAX_STEM_CHARS)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed.chars().all(|c| c == '_') {
        "preset".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Ruta del archivo de un preset con ese nombre dentro de `dir`.
pub fn file_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.json", sanitize_file_stem(name)))
}

/// Guarda `json` como preset `name`, creando la carpeta si hace falta. Escribe a un archivo
/// temporal y lo renombra, así un fallo a medias no corrompe un preset existente.
pub fn save(dir: &Path, name: &str, json: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = file_path(dir, name);
    write_atomic(&path, json)?;
    Ok(path)
}

/// Escribe `json` en `path` por archivo temporal + renombrado.
pub fn write_atomic(path: &Path, json: &str) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Archivos `.json` de `dir`, ordenados sin distinguir mayúsculas. Una carpeta que no existe
/// da una lista vacía (aún no hay presets de usuario).
pub fn list_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort_by_key(|p| {
        p.file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default()
    });
    files
}

pub fn read(path: &Path) -> io::Result<String> {
    std::fs::read_to_string(path)
}

/// Borra un archivo de preset. Sólo acepta rutas dentro de `dir` (comprobado por padre) y con
/// extensión `.json`.
pub fn delete(dir: &Path, path: &Path) -> io::Result<()> {
    let inside = path.parent() == Some(dir) && path.extension().is_some_and(|e| e == "json");
    if !inside {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "el archivo no está en la carpeta de presets",
        ));
    }
    std::fs::remove_file(path)
}

/// `true` si todos los parámetros de `reference` existen en `other` con diferencia <= `tol`
/// (`other` puede tener parámetros de más). Sirve para saber si un preset está modificado.
pub fn params_match(
    reference: &BTreeMap<String, f32>,
    other: &BTreeMap<String, f32>,
    tol: f32,
) -> bool {
    reference
        .iter()
        .all(|(k, v)| other.get(k).is_some_and(|o| (o - v).abs() <= tol))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "analog-suite-test-{}-{tag}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn config_root_prefers_xdg_then_home() {
        assert_eq!(
            config_root_from(Some("/x/cfg"), Some("/home/u")),
            Some(PathBuf::from("/x/cfg/analog-suite"))
        );
        assert_eq!(
            config_root_from(None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/analog-suite"))
        );
        // Variables vacías cuentan como ausentes.
        assert_eq!(
            config_root_from(Some(""), Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/analog-suite"))
        );
        assert_eq!(config_root_from(None, None), None);
        assert_eq!(config_root_from(Some(""), Some("")), None);
    }

    #[test]
    fn sanitize_blocks_path_traversal_and_hidden_files() {
        assert_eq!(sanitize_file_stem("Mi preset 1"), "Mi preset 1");
        assert_eq!(sanitize_file_stem("Voz – Ñandú"), "Voz _ Ñandú");
        for evil in [
            "../../etc/passwd",
            "/abs/path",
            "..",
            ".hidden",
            "a/b\\c",
            "con:col",
        ] {
            let s = sanitize_file_stem(evil);
            assert!(
                !s.contains('/') && !s.contains('\\') && !s.contains('.'),
                "{evil:?} -> {s:?}"
            );
            assert!(!s.starts_with('.'));
        }
        assert_eq!(sanitize_file_stem(""), "preset");
        assert_eq!(sanitize_file_stem("   "), "preset");
        assert_eq!(sanitize_file_stem("///"), "preset");
        assert_eq!(sanitize_file_stem(&"x".repeat(500)).chars().count(), 64);
    }

    #[test]
    fn save_list_read_delete_roundtrip() {
        let dir = temp_dir("roundtrip");
        assert!(
            list_files(&dir).is_empty(),
            "carpeta inexistente = lista vacía"
        );

        let a = save(&dir, "zeta", "{\"n\":1}").unwrap();
        let b = save(&dir, "Alfa", "{\"n\":2}").unwrap();
        assert!(a.starts_with(&dir) && b.starts_with(&dir));

        let files = list_files(&dir);
        assert_eq!(
            files,
            vec![b.clone(), a.clone()],
            "orden alfabético sin mayúsculas"
        );
        assert_eq!(read(&b).unwrap(), "{\"n\":2}");

        // Guardar con el mismo nombre sobrescribe, sin dejar temporales.
        save(&dir, "zeta", "{\"n\":3}").unwrap();
        write_atomic(&b, "{\"n\":9}").unwrap();
        assert_eq!(read(&b).unwrap(), "{\"n\":9}");
        write_atomic(&b, "{\"n\":2}").unwrap();
        assert_eq!(read(&a).unwrap(), "{\"n\":3}");
        assert_eq!(list_files(&dir).len(), 2);
        assert!(std::fs::read_dir(&dir)
            .unwrap()
            .all(|e| e.unwrap().path().extension().unwrap() == "json"));

        delete(&dir, &a).unwrap();
        assert_eq!(list_files(&dir), vec![b]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn evil_names_stay_inside_the_preset_folder() {
        let dir = temp_dir("evil");
        let path = save(&dir, "../../escape", "{}").unwrap();
        assert_eq!(path.parent(), Some(dir.as_path()));
        assert!(!dir.parent().unwrap().join("escape.json").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_ignores_non_json_files_and_subfolders() {
        let dir = temp_dir("ignore");
        save(&dir, "ok", "{}").unwrap();
        std::fs::write(dir.join("notas.txt"), "x").unwrap();
        std::fs::create_dir_all(dir.join("sub.json")).unwrap();
        assert_eq!(list_files(&dir).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_refuses_paths_outside_the_folder() {
        let dir = temp_dir("del");
        let other = temp_dir("del-other");
        let outside = save(&other, "x", "{}").unwrap();
        assert!(delete(&dir, &outside).is_err());
        assert!(outside.exists());
        let not_json = dir.join("a.txt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&not_json, "x").unwrap();
        assert!(delete(&dir, &not_json).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn params_match_compares_reference_keys_within_tolerance() {
        let m = |kv: &[(&str, f32)]| -> BTreeMap<String, f32> {
            kv.iter().map(|(k, v)| (k.to_string(), *v)).collect()
        };
        let reference = m(&[("a", 0.5), ("b", 0.25)]);
        assert!(params_match(
            &reference,
            &m(&[("a", 0.5005), ("b", 0.25), ("extra", 1.0)]),
            2e-3
        ));
        assert!(!params_match(
            &reference,
            &m(&[("a", 0.52), ("b", 0.25)]),
            2e-3
        ));
        assert!(
            !params_match(&reference, &m(&[("a", 0.5)]), 2e-3),
            "falta una clave"
        );
        assert!(params_match(&BTreeMap::new(), &m(&[]), 0.0));
    }
}
