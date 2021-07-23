#![feature(bindings_after_at)]
use anyhow::{Context, Result};
use log::*;
use std::{env, path::PathBuf};
use structopt::StructOpt;

use colored::*;
use std::convert::TryInto;
use tokio::fs;

mod actions;
mod config;
mod file_actions;
mod link;
#[cfg(test)]
mod tests;
mod util;

use config::*;
use link::System;
use util::WritableConfig;

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
        let (system_config_file, system_config) = get_sys_config(self.config_file.as_ref())?;
        let (path, proj_config) = get_project_config(
            self.project_path
                .as_ref()
                .or_else(|| {
                    self.project
                        .clone()
                        .and_then(|y| system_config.projects.get(&y))
                        .map(|x| &x.path)
                })
                .or_else(|| system_config.default.as_ref()),
        )?;

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
impl ProjectContext {
    //    pub fn write_to_file(&self, config: ProjectConfig) -> Result<()> {
    //        let new_toml = toml::to_vec(&final_project_config)?;
    //        fs::write(ctx.project_config_path.join(".links.toml"), new_toml).await?;
    //    }
}

impl Args {
    fn try_to_context(self) -> Result<ProjectContext> {
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
    let command = args.command.clone();
    match command {
        Command::Sync => {
            actions::sync(args.try_into()?).await?;
        }
        Command::Manage { default } => {
            let ctx = args.try_to_context()?;
            let config = actions::manage(&ctx, default).context(format!(
                "Failure managing {}",
                ctx.project_config_path.display()
            ))?;
            fs::write(ctx.system_config_path, toml::to_vec(&config)?)
                .await
                .context("Could not write to system config file")?;
            info!("Managed {}", ctx.project.name);
        }
        Command::Add {
            src,
            destination,
            name,
        } => {
            let ctx = args.try_to_context()?;
            let config = actions::add(&ctx, src, destination, name)
                .await
                .context("Failure adding link")?;
            let data = toml::to_vec(&config)?;
            fs::write(ctx.project_config_path.join(".links.toml"), data).await?;
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
            fs::write(&dir.join(".links.toml"), &text).await?;
        }
        Command::List => {
            let ctx = args.try_to_context()?;
            println!("{} {}", "Links for".bold(), ctx.project.name.bold());
            for link in ctx.project.links {
                print!("{}", link);
            }
        }
        Command::Revert { file } => {
            let ctx = args.try_to_context()?;
            let config = actions::revert(&ctx, &file).await?;
            let text = toml::to_vec(&config)?;
            fs::write(&ctx.project_config_path.join(".links.toml"), &text).await?;
        }
        Command::Prune => {
            let ctx = args.try_to_context()?;
            actions::prune(&ctx)?.write_to_file(&ctx.project_config_path.join(".links.toml"))?;
        }
    };
    Ok(())
}
