use crate::goals::Goal;
use crate::link::{Link, System};
use crate::packages::ProgramConfig;
use crate::util::WritableConfig;
use anyhow::{bail, Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    env, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ProjectConfig {
    pub name: String,
    pub id: String,
    pub default: Option<System>,
    pub systems: Vec<System>,
    pub variables: Option<HashMap<String, String>>,
    pub goals: Option<HashMap<String, Goal>>,
    pub programs: Option<Vec<ProgramConfig>>,
    pub links: Vec<Link>,
}

impl ProjectConfig {
    pub fn remove_start(proj_path: &Path, path: &Path) -> Option<String> {
        Some(path.strip_prefix(proj_path).ok()?.to_str()?.to_string())
    }

    pub fn new(name: String, path: &Path) -> ProjectConfig {
        let mut hasher = DefaultHasher::new();
        name.hash(&mut hasher);
        path.hash(&mut hasher);
        std::time::Instant::now().hash(&mut hasher);
        ProjectConfig {
            default: None,
            name,
            id: format!("{}", hasher.finish()),
            systems: Vec::new(),
            links: Vec::new(),
            variables: None,
            goals: None,
            programs: None,
        }
    }
    pub fn save(&self, ctx: &crate::ProjectContext) -> Result<()> {
        self.write_to_file(&ctx.project_config_path.join(".links.toml"))
    }
}

pub fn get_config_loc() -> Option<PathBuf> {
    ProjectDirs::from("com", "AusCyber", "dotfile-sync").map(|x| x.config_dir().to_path_buf())
}

pub fn get_sys_config(config_path: Option<impl AsRef<Path>>) -> Result<(PathBuf, SystemConfig)> {
    match config_path {
        Some(x) => Ok((
            x.as_ref().to_path_buf(),
            SystemConfig::read_from_file(x.as_ref())?,
        )),
        None => match get_config_loc()
            .context("Failed to get config location")
            .and_then(|x| Ok(x.join("config.toml").canonicalize()?))
        {
            Ok(x) => Ok((x.clone(), SystemConfig::read_from_file(x.as_ref())?)),
            _ => {
                let par_dir = get_config_loc().context("Failed to get config location")?;
                let loc = par_dir.join("config.toml");
                fs::create_dir_all(par_dir)?;
                Ok((loc, SystemConfig::new()))
            }
        },
    }
}

pub fn get_project_config(config_path: Option<&PathBuf>) -> Result<(PathBuf, ProjectConfig)> {
    match config_path {
        Some(x) => {
            if !x.is_file() {
                Ok((
                    x.clone(),
                    ProjectConfig::read_from_file(&x.join(".links.toml"))?,
                ))
            } else {
                Ok((
                    x.parent()
                        .context("Could not get parent folder of config file")
                        .map(Path::to_path_buf)?,
                    ProjectConfig::read_from_file(x)?,
                ))
            }
        }
        None => {
            let proj_path = env::current_dir()?;
            let file_path = proj_path.join(".links.toml");
            if !file_path.exists() {
                bail!("No config file in current directory")
            }
            Ok((proj_path, ProjectConfig::read_from_file(&file_path)?))
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ProjectOutput {
    pub system: Option<System>,
    pub path: PathBuf,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct SystemConfig {
    pub default: Option<PathBuf>,
    pub projects: HashMap<String, ProjectOutput>,
    pub sudo_program: Option<String>,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self::new()
    }
}
impl SystemConfig {
    pub fn new() -> SystemConfig {
        SystemConfig {
            default: None,
            projects: HashMap::new(),
            sudo_program: None,
        }
    }

    pub fn get_project(&self, name: &str) -> Option<&ProjectOutput> {
        self.projects.get(name)
    }

    pub fn add_project(&mut self, name: String, path: PathBuf) {
        self.projects
            .insert(name, ProjectOutput { system: None, path });
    }
}
