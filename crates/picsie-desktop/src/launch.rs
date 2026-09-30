//! Native process arguments; desktop file associations pass positional project paths.
use anyhow::{Result, bail};
use std::{ffi::OsString, path::PathBuf};

pub enum Launch {
    Help,
    Version,
    Measure { moving: bool },
    Projects(Vec<PathBuf>),
}

pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Launch> {
    let mut arguments = arguments.into_iter();
    let mut paths = Vec::new();
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--help" | "-h") => return Ok(Launch::Help),
            Some("--version" | "-V") => return Ok(Launch::Version),
            Some("--measure") => return Ok(Launch::Measure { moving: false }),
            Some("--measure-moving") => return Ok(Launch::Measure { moving: true }),
            Some("--open") => {
                let Some(path) = arguments.next().filter(|path| !path.is_empty()) else {
                    bail!("--open requires a project path");
                };
                if path.to_str().is_some_and(|path| path.starts_with('-')) {
                    bail!("--open requires a project path; use -- before paths beginning with '-'");
                }
                paths.push(PathBuf::from(path));
            }
            Some("--") => {
                paths.extend(arguments.map(PathBuf::from));
                break;
            }
            Some(flag) if flag.starts_with('-') => {
                bail!("Unknown option: {flag}. Run picsie --help.")
            }
            _ => paths.push(PathBuf::from(argument)),
        }
    }
    Ok(Launch::Projects(paths))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn desktop_file_associations_accept_multiple_projects_and_spaces() {
        let Launch::Projects(paths) = parse(args(&["a project.picsie", "old.electropic"])).unwrap()
        else {
            panic!("expected projects");
        };
        assert_eq!(
            paths,
            [
                PathBuf::from("a project.picsie"),
                PathBuf::from("old.electropic")
            ]
        );
    }

    #[test]
    fn old_open_flag_and_option_terminator_remain_supported() {
        let Launch::Projects(paths) =
            parse(args(&["--open", "package.comp", "--", "-name.picsie"])).unwrap()
        else {
            panic!("expected projects");
        };
        assert_eq!(
            paths,
            [PathBuf::from("package.comp"), PathBuf::from("-name.picsie")]
        );
    }

    #[test]
    fn malformed_arguments_fail_instead_of_opening_the_sample() {
        assert!(parse(args(&["--open"])).is_err());
        assert!(parse(args(&["--open", "--version"])).is_err());
        assert!(parse(args(&["--unknown"])).is_err());
    }
}
