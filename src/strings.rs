//! Textos de la interfaz. Idioma único (español).

pub const UPDATED: &str = "Actualizado";

pub const EMPTY_TITLE: &str = "Sin datos";
pub const EMPTY_HINT: &str = "El panel mostrará aquí la información de la aplicación.";
pub const FOOTER: &str = "Clic derecho en el icono para más opciones";

pub const MENU_SHOW: &str = "Mostrar estado";
pub const MENU_REFRESH: &str = "Actualizar ahora";
pub const MENU_TEST_NOTIFICATION: &str = "Probar notificación";
pub const MENU_AUTOSTART: &str = "Iniciar con Windows";
pub const MENU_CONFIG: &str = "Abrir configuración";
pub const MENU_EXIT: &str = "Salir";

pub const NOTIFY_TITLE: &str = "Aviso";
pub const NOTIFY_TEST_BODY: &str = "Notificación de prueba de la bandeja.";

pub const SETTINGS_TITLE: &str = "Configuración";
pub const SETTINGS_REFRESH: &str = "Intervalo de actualización (s)";
pub const SETTINGS_SAVE: &str = "Guardar";
pub const SETTINGS_CANCEL: &str = "Cancelar";
pub const SETTINGS_OPEN_FOLDER: &str = "Abrir carpeta";
pub const SETTINGS_INVALID_REFRESH: &str = "El intervalo debe estar entre 10 y 3600 segundos.";
pub const SETTINGS_SAVE_FAILED: &str = "No se pudo guardar la configuración: {error}";

pub const ROW_TIME: &str = "Hora local";
pub const ROW_TIME_DETAIL: &str = "Se actualiza con el temporizador";

/// Mensaje de error al guardar la configuración.
pub fn save_failed(error: &str) -> String {
    SETTINGS_SAVE_FAILED.replace("{error}", error)
}
