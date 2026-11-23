//! Interpretación de la línea de comandos.

use crate::meta;
use crate::platform::console;

/// Opciones del modo normal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Args {
    pub(crate) show: bool,
    pub(crate) dump: bool,
}

/// Acción que debe ejecutar el programa.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Invocation {
    Run(Args),
    Version,
    Help,
}

/// Interpreta los argumentos, sin el nombre del ejecutable.
///
/// La precedencia es la histórica: versión, después ayuda y por último el modo
/// normal. Los argumentos desconocidos se ignoran.
pub(crate) fn parse(args: &[String]) -> Invocation {
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        return Invocation::Version;
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Invocation::Help;
    }
    Invocation::Run(Args {
        show: args.iter().any(|arg| arg == "--show"),
        dump: args.iter().any(|arg| arg == "--dump"),
    })
}

/// Línea de versión, sin salto de línea.
pub(crate) fn version() -> String {
    format!("{} {}", meta::BIN_NAME, meta::VERSION)
}

/// Texto de ayuda, sin salto de línea final.
pub(crate) fn help() -> String {
    format!(
        "Uso: {} [--show] [--dump] [--version]\n  \
         --show     abre el panel al arrancar\n  \
         --dump     imprime el estado en JSON y termina\n  \
         --version  muestra la versión",
        meta::BIN_NAME
    )
}

/// Escribe la versión en la consola.
pub(crate) fn print_version() {
    console::print_line(&version());
}

/// Escribe la ayuda en la consola.
pub(crate) fn print_help() {
    console::print_line(&help());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn version_flags_are_recognised() {
        assert_eq!(parse(&args(&["--version"])), Invocation::Version);
        assert_eq!(parse(&args(&["-V"])), Invocation::Version);
    }

    #[test]
    fn help_flags_are_recognised() {
        assert_eq!(parse(&args(&["--help"])), Invocation::Help);
        assert_eq!(parse(&args(&["-h"])), Invocation::Help);
    }

    #[test]
    fn version_takes_precedence() {
        assert_eq!(parse(&args(&["--help", "--version"])), Invocation::Version);
        assert_eq!(parse(&args(&["--dump", "--version"])), Invocation::Version);
        assert_eq!(parse(&args(&["--show", "-V", "-h"])), Invocation::Version);
    }

    #[test]
    fn help_takes_precedence_over_run() {
        assert_eq!(parse(&args(&["--dump", "--help"])), Invocation::Help);
        assert_eq!(parse(&args(&["--show", "-h"])), Invocation::Help);
    }

    #[test]
    fn run_collects_show_and_dump() {
        assert_eq!(
            parse(&args(&["--show", "--dump"])),
            Invocation::Run(Args {
                show: true,
                dump: true
            })
        );
        assert_eq!(
            parse(&args(&["--dump"])),
            Invocation::Run(Args {
                show: false,
                dump: true
            })
        );
        assert_eq!(parse(&args(&[])), Invocation::Run(Args::default()));
    }

    #[test]
    fn unknown_and_miscased_arguments_are_ignored() {
        assert_eq!(
            parse(&args(&["-v", "--Show", "extra"])),
            Invocation::Run(Args::default())
        );
    }

    #[test]
    fn version_text_has_no_trailing_newline() {
        let text = version();
        assert_eq!(text, format!("{} {}", meta::BIN_NAME, meta::VERSION));
        assert!(!text.ends_with('\n'));
    }

    #[test]
    fn help_layout_matches_the_cli_contract() {
        let text = help();
        let lines: Vec<&str> = text.split('\n').collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("Uso: "));
        assert!(lines[0].ends_with(" [--show] [--dump] [--version]"));
        assert_eq!(lines[1], "  --show     abre el panel al arrancar");
        assert_eq!(lines[2], "  --dump     imprime el estado en JSON y termina");
        assert_eq!(lines[3], "  --version  muestra la versión");
        assert!(!text.ends_with('\n'));
    }
}
