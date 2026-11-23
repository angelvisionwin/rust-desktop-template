//! Configuración persistente en el directorio de datos de la aplicación.

use std::path::PathBuf;

use serde::Deserialize;

use crate::meta;
use crate::platform::paths;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Intervalo entre actualizaciones, en segundos.
    pub refresh_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            refresh_seconds: 60,
        }
    }
}

impl Config {
    /// Intervalo efectivo, acotado a un rango razonable.
    pub fn refresh_interval_secs(&self) -> u64 {
        self.refresh_seconds.clamp(10, 3600)
    }

    /// Carga la configuración; si no existe, escribe la de por defecto.
    pub fn load() -> Config {
        let path = path();
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Config>(&text) {
                Ok(config) => config,
                Err(error) => {
                    meta::log(&format!("config: {error}; se usan los valores por defecto"));
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
        format!(
            "# Configuración de {app}\n\
             \n\
             # Intervalo de actualización en segundos (10-3600)\n\
             refresh_seconds = {refresh}\n\
             ",
            app = meta::APP_NAME,
            refresh = self.refresh_seconds,
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
            meta::log(&format!("config: no se pudo crear: {error}"));
        }
    }
}

pub fn dir() -> PathBuf {
    paths::data_dir(meta::DATA_DIR_NAME)
}

pub fn path() -> PathBuf {
    dir().join(meta::CONFIG_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_applied_for_missing_keys() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.refresh_interval_secs(), 60);
    }

    #[test]
    fn values_are_clamped() {
        let low: Config = toml::from_str("refresh_seconds = 2").unwrap();
        assert_eq!(low.refresh_interval_secs(), 10);
        let high: Config = toml::from_str("refresh_seconds = 99999").unwrap();
        assert_eq!(high.refresh_interval_secs(), 3600);
    }

    #[test]
    fn rendered_config_round_trips() {
        let config = Config {
            refresh_seconds: 120,
        };
        let parsed: Config = toml::from_str(&config.render()).unwrap();
        assert_eq!(parsed.refresh_interval_secs(), 120);
    }
}
