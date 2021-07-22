#![feature(bindings_after_at)]
use anyhow::{Context, Result};
use log::*;
use std::{env, fs, path::PathBuf};
use structopt::StructOpt;

use std::convert::TryInto;
use std::sync::Arc;

mod actions;
mod config;
mod file_actions;
mod link;
#[cfg(test)]
mod tests;
mod util;

use config::*;
use link::System;

#[derive(StructOpt, Clone)]
#[structopt(about = "Manage dotfiles")]
pub struct Args {
    #[structopt(short, long)]
    #[structopt(long, about = "Location of system config file")]
    config_file: Option<PathBuf>,
    #[structopt(long)]
    project_path: Option<PathBuf>,
    #[structopt(long, short, about = "Locate project from system projects")]
    project: Option<String>,
    #[structopt(long, short)]
    system: Option<System>,
    #[structopt(subcommand)]
    command: Command,
}

pub struct ProjectContext {
    pub args: Args,
    pub project: ProjectConfig,
    pub project_config_path: PathBuf,
    pub system_config: SystemConfig,
    pub system_config_path: PathBuf,
    pub system: Option<System>,
}
impl TryInto<ProjectContext> for Args {
    type Error = anyhow::Error;
    fn try_into(self) -> Result<ProjectContext> {
        let (system_config_file, system_config) =
            get_sys_config(self.config_file.as_ref())?.to_owned();
        let (path, proj_config) = get_project_config(
            self.project_path
                .as_ref()
                .or_else(|| {
                    self.project
                        .clone()
                        .and_then(|y| system_config.projects.get(&y))
                        .map(|x| &x.path)
                })
                .or(system_config.default.as_ref())
                .clone(),
        )?
        .to_owned();

        let system = self
            .system
            .as_ref()
            .or_else(|| {
                system_config
                    .get_project(&proj_config.name)?
                    .system
                    .as_ref()
            })
            .or_else(|| proj_config.default.as_ref())
            .cloned();
        Ok(ProjectContext {
            //            command: self.command.clone(),
            args: self,
            project: proj_config,
            project_config_path: path,
            system_config,
            system_config_path: system_config_file,
            system,
        })
    }
}

impl Args {
    fn to_context(self) -> Result<ProjectContext> {
        self.try_into()
    }
}

#[derive(StructOpt, Clone)]
enum Command {
    Sync,
    Add {
        src: String,
        destination: Option<String>,
        #[structopt(short, long)]
        name: Option<String>,
    },
    Init {
        name: Option<String>,
    },
    Revert {
        file: PathBuf,
    },
    Manage {
        #[structopt(short, long)]
        default: bool,
    },
    Prune,
    List,
}

#[tokio::main]
pub async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::from_args();
    let Args {
        project_path,
        project,
        system,
        config_file,
        command,
    } = args.clone();

    match command {
        Command::Sync => {
            actions::sync(args.try_into()?).await?;
        }
        Command::Manage { default } => {
            let ctx = args.to_context()?;
            let config = actions::manage(&ctx, default).context(format!(
                "Failure managing {}",
                ctx.project_config_path.display()
            ))?;
            fs::write(ctx.system_config_path, toml::to_vec(&config)?)
                .context("Could not write to system config file")?;
            info!("Managed {}", ctx.project.name);
        }
        Command::Add {
            src,
            destination,
            name,
        } => {
            let ctx = args.to_context()?;
            let new_config = actions::add(&ctx, src, destination, name)
                .await
                .context("Failure adding link")?;
            let new_toml = toml::to_vec(&new_config)?;
            fs::write(ctx.project_config_path.join(".links.toml"), new_toml)?;
            info!("Added {}", ctx.project.name);
        }
        Command::Init { name } => {
            let dir = env::current_dir()?;
            let project = ProjectConfig::new(
                name.unwrap_or(
                    dir.file_name()
                        .and_then(|x| x.to_str())
                        .map(|x| x.into())
                        .context("Invalid name")?,
                ),
                &dir,
            );
            let text = toml::to_vec(&project)?;
            fs::write(&dir.join(".links.toml"), &text)?;
        }
        Command::List => {
            let (_, sys_config) = get_sys_config(config_file)?;
            let (_, proj) = get_project_config(
                project_path
                    .or_else(|| {
                        project.and_then(|y| sys_config.projects.get(&y).map(|x| x.path.clone()))
                    })
                    .or(sys_config.default)
                    .as_ref(),
            )?;

            for link in proj.links {
                println!("{:?}", link);
            }
        }
        Command::Revert { file } => {
            let (proj_path, proj) = get_project_config(
                project_path
                    .or_else(|| {
                        project.and_then(|y| {
                            get_sys_config(config_file.clone())
                                .ok()?
                                .1
                                .projects
                                .get(&y)
                                .map(|x| x.path.clone())
                        })
                    })
                    .or_else(|| get_sys_config(config_file.clone()).ok()?.1.default)
                    .as_ref(),
            )
            .context("Could not find project_path")?;
            let system = system
                .or_else(|| {
                    get_sys_config(config_file)
                        .ok()?
                        .1
                        .projects
                        .get(&proj.name)?
                        .clone()
                        .system
                })
                .or(proj.default.clone());
            let config = actions::revert(file, proj, &proj_path, system)?;
            let text = toml::to_vec(&config)?;
            fs::write(&proj_path.join(".links.toml"), &text)?;
        }
        Command::Prune => {
            let (_, sys_config) = get_sys_config(config_file)?;
            let (proj_path, proj) = get_project_config(
                project_path
                    .or_else(|| {
                        project.and_then(|y| sys_config.projects.get(&y).map(|x| x.path.clone()))
                    })
                    .or(sys_config.default)
                    .as_ref(),
            )?;
            let text = toml::to_vec(&actions::prune(proj_path.clone(), proj))?;
            fs::write(&proj_path.join(".links.toml"), &text)?;
        }
    };
    Ok(())
}
