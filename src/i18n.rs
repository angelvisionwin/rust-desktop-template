//! Traducciones de la interfaz. El español es el idioma de referencia.

use std::fmt;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    Ca,
    En,
    Fr,
    De,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "es",
            Lang::Ca => "ca",
            Lang::En => "en",
            Lang::Fr => "fr",
            Lang::De => "de",
        }
    }

    pub fn parse(code: &str) -> Option<Lang> {
        let code = code.trim().to_ascii_lowercase();
        let base = code.split(['-', '_']).next().unwrap_or("");
        match base {
            "es" | "spa" => Some(Lang::Es),
            "ca" | "cat" => Some(Lang::Ca),
            "en" | "eng" => Some(Lang::En),
            "fr" | "fra" | "fre" => Some(Lang::Fr),
            "de" | "deu" | "ger" => Some(Lang::De),
            _ => None,
        }
    }

    /// Idioma del sistema, con español como respaldo.
    pub fn system() -> Lang {
        let id = crate::util::user_ui_language();
        match id & 0x3FF {
            0x03 => Lang::Ca,
            0x07 => Lang::De,
            0x09 => Lang::En,
            0x0C => Lang::Fr,
            0x0A => Lang::Es,
            _ => Lang::Es,
        }
    }
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug)]
pub struct Strings {
    pub app: &'static str,
    pub updated: &'static str,
    pub empty_title: &'static str,
    pub empty_hint: &'static str,
    pub footer: &'static str,
    pub kind_keyboard: &'static str,
    pub kind_mouse: &'static str,
    pub kind_gamepad: &'static str,
    pub kind_headset: &'static str,
    pub kind_other: &'static str,
    pub charge_charging: &'static str,
    pub charge_finishing: &'static str,
    pub charge_full: &'static str,
    pub charge_cable: &'static str,
    pub menu_show: &'static str,
    pub menu_refresh: &'static str,
    pub menu_autostart: &'static str,
    pub menu_config: &'static str,
    pub menu_exit: &'static str,
    pub low_title: &'static str,
    pub low_body: &'static str,
    pub tip_no_devices: &'static str,
    pub tip_charging: &'static str,
    pub remaining: &'static str,
    pub approx: &'static str,
    pub unit_day: &'static str,
    pub unit_hour: &'static str,
    pub unit_minute: &'static str,
    pub settings_title: &'static str,
    pub settings_language: &'static str,
    pub settings_auto: &'static str,
    pub settings_threshold: &'static str,
    pub settings_refresh: &'static str,
    pub settings_show_remaining: &'static str,
    pub settings_ignored: &'static str,
    pub settings_ignored_hint: &'static str,
    pub settings_save: &'static str,
    pub settings_cancel: &'static str,
    pub settings_open_folder: &'static str,
    pub settings_error_title: &'static str,
    pub settings_invalid_threshold: &'static str,
    pub settings_invalid_refresh: &'static str,
    pub settings_save_failed: &'static str,
}

impl Strings {
    pub fn kind(&self, kind: crate::battery::DeviceKind) -> &'static str {
        use crate::battery::DeviceKind;
        match kind {
            DeviceKind::Keyboard => self.kind_keyboard,
            DeviceKind::Mouse => self.kind_mouse,
            DeviceKind::Gamepad => self.kind_gamepad,
            DeviceKind::Headset => self.kind_headset,
            DeviceKind::Other => self.kind_other,
        }
    }

    pub fn charge(&self, state: crate::battery::ChargeState) -> &'static str {
        use crate::battery::ChargeState;
        match state {
            ChargeState::Charging => self.charge_charging,
            ChargeState::Finishing => self.charge_finishing,
            ChargeState::Full => self.charge_full,
        }
    }

    pub fn updated_at(&self, hours: u16, minutes: u16) -> String {
        format!("{} {hours:02}:{minutes:02}", self.updated)
    }

    pub fn low_body(&self, name: &str, percent: u8) -> String {
        self.low_body
            .replace("{name}", name)
            .replace("{percent}", &percent.to_string())
    }

    pub fn remaining_text(&self, duration: Duration) -> String {
        let time = self.format_duration(duration);
        self.remaining.replace("{time}", &time)
    }

    pub fn save_failed(&self, error: &str) -> String {
        self.settings_save_failed.replace("{error}", error)
    }

    pub fn format_duration(&self, duration: Duration) -> String {
        let total_minutes = duration.as_secs() / 60;
        let days = total_minutes / (60 * 24);
        let hours = (total_minutes % (60 * 24)) / 60;
        let minutes = total_minutes % 60;
        if days > 0 && hours > 0 {
            format!("{days} {} {hours} {}", self.unit_day, self.unit_hour)
        } else if days > 0 {
            format!("{days} {}", self.unit_day)
        } else if hours > 0 && minutes > 0 {
            format!("{hours} {} {minutes} {}", self.unit_hour, self.unit_minute)
        } else if hours > 0 {
            format!("{hours} {}", self.unit_hour)
        } else {
            format!("{minutes} {}", self.unit_minute)
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Fija el idioma de la interfaz para el resto del proceso.
pub fn set_current(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Lang::Ca,
        2 => Lang::En,
        3 => Lang::Fr,
        4 => Lang::De,
        _ => Lang::Es,
    }
}

/// Textos del idioma activo.
pub fn text() -> &'static Strings {
    strings(current())
}

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::Es => &ES,
        Lang::Ca => &CA,
        Lang::En => &EN,
        Lang::Fr => &FR,
        Lang::De => &DE,
    }
}

pub static ES: Strings = Strings {
    app: "Batería",
    updated: "Actualizado",
    empty_title: "Sin dispositivos con batería",
    empty_hint: "Conecta un teclado, ratón, mando o auriculares.",
    footer: "Clic derecho en el icono para más opciones",
    kind_keyboard: "Teclado",
    kind_mouse: "Ratón",
    kind_gamepad: "Mando",
    kind_headset: "Auriculares",
    kind_other: "Dispositivo",
    charge_charging: "Cargando",
    charge_finishing: "Carga final",
    charge_full: "Carga completa",
    charge_cable: "Cable",
    menu_show: "Mostrar estado",
    menu_refresh: "Actualizar ahora",
    menu_autostart: "Iniciar con Windows",
    menu_config: "Abrir configuración",
    menu_exit: "Salir",
    low_title: "Batería baja",
    low_body: "{name} al {percent}%",
    tip_no_devices: "sin dispositivos",
    tip_charging: "cargando",
    remaining: "quedan {time}",
    approx: "≈",
    unit_day: "d",
    unit_hour: "h",
    unit_minute: "min",
    settings_title: "Configuración",
    settings_language: "Idioma",
    settings_auto: "Automático (sistema)",
    settings_threshold: "Aviso de batería baja (%)",
    settings_refresh: "Actualizar cada (s)",
    settings_show_remaining: "Mostrar el tiempo restante estimado",
    settings_ignored: "Dispositivos ignorados (uno por línea)",
    settings_ignored_hint: "Nombre o clave del dispositivo; se oculta al coincidir.",
    settings_save: "Guardar",
    settings_cancel: "Cancelar",
    settings_open_folder: "Abrir carpeta",
    settings_error_title: "Valores no válidos",
    settings_invalid_threshold: "El umbral debe estar entre 1 y 100 %.",
    settings_invalid_refresh: "El intervalo debe estar entre 10 y 3600 segundos.",
    settings_save_failed: "No se pudo guardar config.toml: {error}",
};

pub static CA: Strings = Strings {
    app: "Bateria",
    updated: "Actualitzat",
    empty_title: "Sense dispositius amb bateria",
    empty_hint: "Connecta un teclat, ratolí, comandament o auriculars.",
    footer: "Clic dret a la icona per a més opcions",
    kind_keyboard: "Teclat",
    kind_mouse: "Ratolí",
    kind_gamepad: "Comandament",
    kind_headset: "Auriculars",
    kind_other: "Dispositiu",
    charge_charging: "S'està carregant",
    charge_finishing: "Càrrega final",
    charge_full: "Càrrega completa",
    charge_cable: "Cable",
    menu_show: "Mostra l'estat",
    menu_refresh: "Actualitza ara",
    menu_autostart: "Inicia amb Windows",
    menu_config: "Obre la configuració",
    menu_exit: "Surt",
    low_title: "Bateria baixa",
    low_body: "{name} al {percent}%",
    tip_no_devices: "sense dispositius",
    tip_charging: "s'està carregant",
    remaining: "queden {time}",
    approx: "≈",
    unit_day: "d",
    unit_hour: "h",
    unit_minute: "min",
    settings_title: "Configuració",
    settings_language: "Idioma",
    settings_auto: "Automàtic (sistema)",
    settings_threshold: "Avís de bateria baixa (%)",
    settings_refresh: "Actualitza cada (s)",
    settings_show_remaining: "Mostra el temps restant estimat",
    settings_ignored: "Dispositius ignorats (un per línia)",
    settings_ignored_hint: "Nom o clau del dispositiu; s'amaga quan coincideix.",
    settings_save: "Desa",
    settings_cancel: "Cancel·la",
    settings_open_folder: "Obre la carpeta",
    settings_error_title: "Valors no vàlids",
    settings_invalid_threshold: "El llindar ha d'estar entre 1 i 100 %.",
    settings_invalid_refresh: "L'interval ha d'estar entre 10 i 3600 segons.",
    settings_save_failed: "No s'ha pogut desar config.toml: {error}",
};

pub static EN: Strings = Strings {
    app: "Battery",
    updated: "Updated",
    empty_title: "No devices with a battery",
    empty_hint: "Connect a keyboard, mouse, controller or headset.",
    footer: "Right-click the icon for more options",
    kind_keyboard: "Keyboard",
    kind_mouse: "Mouse",
    kind_gamepad: "Controller",
    kind_headset: "Headset",
    kind_other: "Device",
    charge_charging: "Charging",
    charge_finishing: "Finishing charge",
    charge_full: "Fully charged",
    charge_cable: "Cable",
    menu_show: "Show status",
    menu_refresh: "Refresh now",
    menu_autostart: "Start with Windows",
    menu_config: "Open settings",
    menu_exit: "Exit",
    low_title: "Low battery",
    low_body: "{name} at {percent}%",
    tip_no_devices: "no devices",
    tip_charging: "charging",
    remaining: "{time} left",
    approx: "≈",
    unit_day: "d",
    unit_hour: "h",
    unit_minute: "min",
    settings_title: "Settings",
    settings_language: "Language",
    settings_auto: "Automatic (system)",
    settings_threshold: "Low battery warning (%)",
    settings_refresh: "Refresh every (s)",
    settings_show_remaining: "Show estimated time remaining",
    settings_ignored: "Ignored devices (one per line)",
    settings_ignored_hint: "Device name or key; the device is hidden when it matches.",
    settings_save: "Save",
    settings_cancel: "Cancel",
    settings_open_folder: "Open folder",
    settings_error_title: "Invalid values",
    settings_invalid_threshold: "The threshold must be between 1 and 100%.",
    settings_invalid_refresh: "The interval must be between 10 and 3600 seconds.",
    settings_save_failed: "Could not save config.toml: {error}",
};

pub static FR: Strings = Strings {
    app: "Batterie",
    updated: "Actualisé",
    empty_title: "Aucun appareil avec batterie",
    empty_hint: "Connectez un clavier, une souris, une manette ou un casque.",
    footer: "Clic droit sur l'icône pour plus d'options",
    kind_keyboard: "Clavier",
    kind_mouse: "Souris",
    kind_gamepad: "Manette",
    kind_headset: "Casque",
    kind_other: "Appareil",
    charge_charging: "En charge",
    charge_finishing: "Fin de charge",
    charge_full: "Charge terminée",
    charge_cable: "Câble",
    menu_show: "Afficher l'état",
    menu_refresh: "Actualiser",
    menu_autostart: "Démarrer avec Windows",
    menu_config: "Ouvrir les paramètres",
    menu_exit: "Quitter",
    low_title: "Batterie faible",
    low_body: "{name} à {percent} %",
    tip_no_devices: "aucun appareil",
    tip_charging: "en charge",
    remaining: "{time} restantes",
    approx: "≈",
    unit_day: "j",
    unit_hour: "h",
    unit_minute: "min",
    settings_title: "Paramètres",
    settings_language: "Langue",
    settings_auto: "Automatique (système)",
    settings_threshold: "Alerte de batterie faible (%)",
    settings_refresh: "Actualiser toutes les (s)",
    settings_show_remaining: "Afficher le temps restant estimé",
    settings_ignored: "Appareils ignorés (un par ligne)",
    settings_ignored_hint: "Nom ou clé de l'appareil ; il est masqué en cas de correspondance.",
    settings_save: "Enregistrer",
    settings_cancel: "Annuler",
    settings_open_folder: "Ouvrir le dossier",
    settings_error_title: "Valeurs non valides",
    settings_invalid_threshold: "Le seuil doit être compris entre 1 et 100 %.",
    settings_invalid_refresh: "L'intervalle doit être compris entre 10 et 3600 secondes.",
    settings_save_failed: "Impossible d'enregistrer config.toml : {error}",
};

pub static DE: Strings = Strings {
    app: "Batterie",
    updated: "Aktualisiert",
    empty_title: "Keine Geräte mit Akku",
    empty_hint: "Verbinde eine Tastatur, Maus, einen Controller oder Kopfhörer.",
    footer: "Rechtsklick auf das Symbol für mehr Optionen",
    kind_keyboard: "Tastatur",
    kind_mouse: "Maus",
    kind_gamepad: "Controller",
    kind_headset: "Kopfhörer",
    kind_other: "Gerät",
    charge_charging: "Wird geladen",
    charge_finishing: "Lädt nach",
    charge_full: "Voll geladen",
    charge_cable: "Kabel",
    menu_show: "Status anzeigen",
    menu_refresh: "Jetzt aktualisieren",
    menu_autostart: "Mit Windows starten",
    menu_config: "Einstellungen öffnen",
    menu_exit: "Beenden",
    low_title: "Akku schwach",
    low_body: "{name} bei {percent} %",
    tip_no_devices: "keine Geräte",
    tip_charging: "wird geladen",
    remaining: "noch {time}",
    approx: "≈",
    unit_day: "T",
    unit_hour: "Std",
    unit_minute: "Min",
    settings_title: "Einstellungen",
    settings_language: "Sprache",
    settings_auto: "Automatisch (System)",
    settings_threshold: "Warnung bei schwachem Akku (%)",
    settings_refresh: "Aktualisieren alle (s)",
    settings_show_remaining: "Geschätzte Restzeit anzeigen",
    settings_ignored: "Ignorierte Geräte (eines pro Zeile)",
    settings_ignored_hint: "Name oder Schlüssel des Geräts; bei Übereinstimmung wird es ausgeblendet.",
    settings_save: "Speichern",
    settings_cancel: "Abbrechen",
    settings_open_folder: "Ordner öffnen",
    settings_error_title: "Ungültige Werte",
    settings_invalid_threshold: "Der Schwellenwert muss zwischen 1 und 100 % liegen.",
    settings_invalid_refresh: "Das Intervall muss zwischen 10 und 3600 Sekunden liegen.",
    settings_save_failed: "config.toml konnte nicht gespeichert werden: {error}",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_language_codes() {
        assert_eq!(Lang::parse("es"), Some(Lang::Es));
        assert_eq!(Lang::parse("ES-es"), Some(Lang::Es));
        assert_eq!(Lang::parse(" ca_ES "), Some(Lang::Ca));
        assert_eq!(Lang::parse("en-GB"), Some(Lang::En));
        assert_eq!(Lang::parse("fr"), Some(Lang::Fr));
        assert_eq!(Lang::parse("de-AT"), Some(Lang::De));
        assert_eq!(Lang::parse("auto"), None);
        assert_eq!(Lang::parse("pt"), None);
    }

    #[test]
    fn formats_duration_by_magnitude() {
        let es = strings(Lang::Es);
        assert_eq!(es.format_duration(Duration::from_secs(35 * 60)), "35 min");
        assert_eq!(es.format_duration(Duration::from_secs(17 * 3600)), "17 h");
        assert_eq!(es.format_duration(Duration::from_secs(2 * 86400)), "2 d");
        assert_eq!(
            es.format_duration(Duration::from_secs(3 * 3600 + 12 * 60)),
            "3 h 12 min"
        );
        assert_eq!(
            es.format_duration(Duration::from_secs(2 * 86400 + 5 * 3600)),
            "2 d 5 h"
        );
    }

    #[test]
    fn every_language_has_all_texts() {
        for lang in [Lang::Es, Lang::Ca, Lang::En, Lang::Fr, Lang::De] {
            let text = strings(lang);
            for value in [
                text.app,
                text.updated,
                text.empty_title,
                text.empty_hint,
                text.footer,
                text.kind_keyboard,
                text.kind_mouse,
                text.kind_gamepad,
                text.kind_headset,
                text.kind_other,
                text.charge_charging,
                text.charge_finishing,
                text.charge_full,
                text.charge_cable,
                text.menu_show,
                text.menu_refresh,
                text.menu_autostart,
                text.menu_config,
                text.menu_exit,
                text.low_title,
                text.low_body,
                text.tip_no_devices,
                text.tip_charging,
                text.remaining,
                text.unit_day,
                text.unit_hour,
                text.unit_minute,
                text.settings_title,
                text.settings_language,
                text.settings_auto,
                text.settings_threshold,
                text.settings_refresh,
                text.settings_show_remaining,
                text.settings_ignored,
                text.settings_ignored_hint,
                text.settings_save,
                text.settings_cancel,
                text.settings_open_folder,
                text.settings_error_title,
                text.settings_invalid_threshold,
                text.settings_invalid_refresh,
            ] {
                assert!(!value.trim().is_empty(), "texto vacío en {lang}");
            }
            assert!(
                text.settings_save_failed.contains("{error}"),
                "falta {{error}} en {lang}"
            );
            assert!(text.low_body.contains("{name}"), "falta {{name}} en {lang}");
            assert!(
                text.low_body.contains("{percent}"),
                "falta {{percent}} en {lang}"
            );
            assert!(
                text.remaining.contains("{time}"),
                "falta {{time}} en {lang}"
            );
        }
    }

    #[test]
    fn low_battery_body_interpolates() {
        assert_eq!(strings(Lang::En).low_body("Mouse", 15), "Mouse at 15%");
        assert_eq!(strings(Lang::Es).low_body("Ratón", 15), "Ratón al 15%");
    }

    #[test]
    fn save_error_interpolates() {
        assert_eq!(
            strings(Lang::Es).save_failed("disco lleno"),
            "No se pudo guardar config.toml: disco lleno"
        );
    }
}
