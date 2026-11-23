//! Configuración persistente en el directorio de datos de la aplicación.

use std::path::PathBuf;

use serde::Deserialize;

use crate::i18n::Lang;
use crate::meta;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `auto`, `es`, `ca`, `en`, `fr` o `de`.
    pub language: String,
    /// Porcentaje a partir del cual se avisa de batería baja.
    pub low_battery_threshold: u8,
    /// Intervalo entre lecturas periódicas, en segundos.
    pub refresh_seconds: u64,
    /// Mostrar el tiempo restante estimado.
    pub show_time_remaining: bool,
    /// Nombres o claves de dispositivos que se ocultan.
    pub ignored_devices: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            language: "auto".to_string(),
            low_battery_threshold: 20,
            refresh_seconds: 60,
            show_time_remaining: true,
            ignored_devices: Vec::new(),
        }
    }
}

impl Config {
    pub fn language(&self) -> Lang {
        Lang::parse(&self.language).unwrap_or_else(Lang::system)
    }

    pub fn threshold(&self) -> u8 {
        self.low_battery_threshold.clamp(1, 100)
    }

    pub fn refresh_interval_secs(&self) -> u64 {
        self.refresh_seconds.clamp(10, 3600)
    }

    pub fn is_ignored(&self, name: &str, key: &str) -> bool {
        self.ignored_devices.iter().any(|pattern| {
            let pattern = pattern.trim();
            !pattern.is_empty()
                && (name.eq_ignore_ascii_case(pattern)
                    || key.eq_ignore_ascii_case(pattern)
                    || name.to_lowercase().contains(&pattern.to_lowercase()))
        })
    }

    /// Carga la configuración; si no existe, escribe la de por defecto.
    pub fn load() -> Config {
        let path = path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => config,
                Err(error) => {
                    crate::util::log(&format!("config: {error}; se usan los valores por defecto"));
                    Config::default()
                }
            },
            Err(_) => {
                let config = Config::default();
                config.write_default(&path);
                config
            }
        }
    }

    /// Texto TOML con comentarios listo para escribir en disco.
    pub fn render(&self) -> String {
        let ignored = self
            .ignored_devices
            .iter()
            .map(|pattern| format!("\"{}\"", escape(pattern)))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "# Configuración de {app}\n\
             # Idioma: auto | es | ca | en | fr | de\n\
             language = \"{language}\"\n\
             \n\
             # Aviso de batería baja (porcentaje)\n\
             low_battery_threshold = {threshold}\n\
             \n\
             # Intervalo de actualización en segundos (10-3600)\n\
             refresh_seconds = {refresh}\n\
             \n\
             # Mostrar tiempo restante estimado\n\
             show_time_remaining = {show}\n\
             \n\
             # Dispositivos a ocultar (nombre o clave), por ejemplo:\n\
             # ignored_devices = [\"Mando Xbox 1\"]\n\
             ignored_devices = [{ignored}]\n\
             ",
            language = escape(&self.language),
            threshold = self.low_battery_threshold,
            refresh = self.refresh_seconds,
            show = self.show_time_remaining,
            app = meta::APP_NAME,
        )
    }

    /// Escribe la configuración en `config.toml`.
    pub fn save(&self) -> std::io::Result<()> {
        let path = path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.render())
    }

    fn write_default(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(path, self.render()) {
            crate::util::log(&format!("config: no se pudo crear: {error}"));
        }
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

pub fn dir() -> PathBuf {
    crate::platform::paths::data_dir(meta::DATA_DIR_NAME)
}

pub fn path() -> PathBuf {
    dir().join(meta::CONFIG_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_applied_for_missing_keys() {
        let config: Config = toml::from_str("low_battery_threshold = 35").unwrap();
        assert_eq!(config.threshold(), 35);
        assert_eq!(config.language, "auto");
        assert_eq!(config.refresh_interval_secs(), 60);
        assert!(config.show_time_remaining);
    }

    #[test]
    fn values_are_clamped() {
        let config: Config =
            toml::from_str("low_battery_threshold = 0\nrefresh_seconds = 2\n").unwrap();
        assert_eq!(config.threshold(), 1);
        assert_eq!(config.refresh_interval_secs(), 10);
    }

    #[test]
    fn language_falls_back_to_system() {
        let config = Config::default();
        assert_eq!(config.language(), Lang::system());
        let config: Config = toml::from_str("language = \"de\"").unwrap();
        assert_eq!(config.language(), Lang::De);
    }

    #[test]
    fn matches_ignored_devices() {
        let config: Config =
            toml::from_str("ignored_devices = [\"Mando Xbox 1\", \"ble:abc\"]\n").unwrap();
        assert!(config.is_ignored("Mando Xbox 1", "xinput:0"));
        assert!(config.is_ignored("mando xbox 1", "xinput:0"));
        assert!(config.is_ignored("Cualquiera", "ble:abc"));
        assert!(!config.is_ignored("MX Master 3S", "ble:xyz"));
        let empty: Config = toml::from_str("ignored_devices = [\"  \"]").unwrap();
        assert!(!empty.is_ignored("MX Master 3S", "ble:xyz"));
    }

    #[test]
    fn rendered_config_round_trips() {
        let config = Config {
            language: "fr".to_string(),
            low_battery_threshold: 30,
            refresh_seconds: 120,
            show_time_remaining: false,
            ignored_devices: vec!["Mando Xbox 1".to_string(), "ble:\"abc\"\\".to_string()],
        };
        let parsed: Config = toml::from_str(&config.render()).unwrap();
        assert_eq!(parsed.language, "fr");
        assert_eq!(parsed.threshold(), 30);
        assert_eq!(parsed.refresh_interval_secs(), 120);
        assert!(!parsed.show_time_remaining);
        assert_eq!(parsed.ignored_devices, config.ignored_devices);
    }

    #[test]
    fn rendered_config_keeps_empty_list() {
        let parsed: Config = toml::from_str(&Config::default().render()).unwrap();
        assert!(parsed.ignored_devices.is_empty());
        assert_eq!(parsed.language, "auto");
        assert!(parsed.show_time_remaining);
    }
}
