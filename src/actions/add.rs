use crate::{config::ProjectConfig, file_actions::recurse_copy, link::*, ProjectContext};
use cascade::cascade;
use itertools::Itertools;
use log::*;
use snafu::Snafu;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Snafu)] 
enum AddError {
    #[snafu(display("No files defined to link"))]
    NoFiles,
    #[snafu(display("Destination {} already exists",path))]
    DestinationAlreadyExists {
       path: VariablePath 
    },
    #[snafu(display("Links already contain link named {}",name))]
    DuplicateLink {
        name: String
    }
}

pub async fn add(
    ctx: &ProjectContext,
    //File to copy
    original_locations: Vec<String>,
    //Location of where to place it in the project
    destination: Option<String>,
    name: Option<String>,
) -> Result<ProjectConfig,AddError> {
    if original_locations.is_empty() {
        return Err(AddError::NoFiles)
    }
    if original_locations.len() == 1 {
        add_individual_link(
            ctx,
            original_locations.first().unwrap().clone(),
            destination,
            name,
        )
        .await
    } else {
        manage_list(ctx, original_locations, destination).await
    }
}

async fn add_individual_link(
    ctx: &ProjectContext,
    mut original_location: String,
    destination: Option<String>,
    name: Option<String>,
) -> Result<ProjectConfig,AddError> {
    //Append current directory if it is a generic location
    let original_location = {
        let p = PathBuf::from(&original_location);
        if p.exists() && !p.has_root() {
            original_location = format!(
                "{}/{}",
                std::env::current_dir()?.display(),
                &original_location
            )
        };
        VariablePath::from(original_location)
    };

    //clean and realise path
    let original_location_cleaned = original_location
        .to_path_buf(ctx.project.variables.as_ref())?
        .canonicalize()
        .context(format!(
            r#"file "{}" could not be found"#,
            &original_location
        ))?;

    let output_dest = match destination.map(PathBuf::from) {
        Some(destination) => {
            let mut output = match destination.strip_prefix(&ctx.project_config_path) {
                Ok(x) => x.to_path_buf(),
                _ => destination,
            };
            if ctx.project_config_path.join(&output).is_dir() {
                output = output.join(original_location_cleaned.file_name().context(format!(
                    "Could not get filename for {}",
                    original_location_cleaned.to_str().unwrap()
                ))?)
            }
            output.to_string_lossy().to_string()
        }

        None => original_location_cleaned
            .file_name()
            .map(|x| x.to_string_lossy().into())
            .context("Could not get file name")?,
    };

    snafu::ensure!(
        !(ctx
            .project
            .links
            .iter()
            .filter_map(|x| same_file::is_same_file(
                &original_location_cleaned,
                &x.destination
                    .to_path_buf(ctx.project.variables.as_ref())
                    .ok()?
            )
            .ok())
            .any(|x| x)
            || ctx
                .project_config_path
                .join(&output_dest)
                .canonicalize()
                .map(|x| {
                    println!("{}", x.display());
                    x.exists()
                })
                .unwrap_or(false)
            || ctx
                .project
                .links
                .iter()
                .any(|x| x.src.contains_path(&output_dest))),
        DestinationAlreadyExistsSnafu{ path: original_location }
    );

    let name = name.unwrap_or(
        original_location_cleaned
            .file_name()
            .map(|x| x.to_string_lossy().into())
            .context("Could not get file name")?,
    );
    snafu::ensure!(
        !ctx.project.links.iter().any(|x| x.name == name),
        DuplicateLinkSnafu { name }
    );

    let get_system = || ctx.args.system.to_owned().context("could not get system");
    let mut found = false;
    let mut completed_links = ctx
        .project
        .links
        .iter()
        .map(|link| {
            if link
                .destination
                .to_path_buf(ctx.project.variables.as_ref())
                .and_then(|x| Ok(x.canonicalize()? != original_location_cleaned))
                .unwrap_or(true)
            {
                return Ok(link.clone());
            }
            found = true;

            let mut link = link.clone();
            let sys = get_system()?;
            link.src = link.src.insert_link(&sys, &output_dest)?;
            Ok(link)
        })
        .collect::<Result<Vec<_>, AddError>>()?;

    if !found {
        let source = SourceFile::Source {
            system: ctx.args.system.clone(),
            src: output_dest.clone(),
        };
        debug!("name is orig: {}, source: {}", original_location, source);
        completed_links.push(Link::new(name.clone(), original_location, source));
    };

    let output_dest = ctx.project_config_path.join(output_dest);
    let final_project_config = cascade! {
        ctx.project.clone();
        ..links = completed_links;
    };
    fs::create_dir_all(
        PathBuf::from(&output_dest)
            .parent()
            .context("Could not get parent folder")?,
    )
    .await?;
    move_link(&original_location_cleaned, &output_dest).await?;
    info!("Added {}", name);
    Ok(final_project_config)
}

async fn manage_list(
    ctx: &ProjectContext,
    locations: Vec<String>,
    destination: Option<String>,
) -> Result<ProjectConfig,AddError> {
    let dest = destination.unwrap_or_else(|| String::from("."));
    fs::create_dir_all(ctx.project_config_path.join(&dest)).await?;
    let mut triples: Vec<_> = locations
        .into_iter()
        .dedup()
        .map(|mut path| {
            let p = PathBuf::from(&path);
            if p.exists() && !p.has_root() {
                path = format!("{}/{}", std::env::current_dir()?.display(), path)
            };
            let variable_path: VariablePath = path.clone().into();
            let cleaned = variable_path
                .to_path_buf(ctx.project.variables.as_ref())?
                .canonicalize()
                .context(format!(r#"file "{}" could not be found"#, &path))?;

            let file_name: String = cleaned
                .file_name()
                .map(|x| x.to_string_lossy())
                .context("Could not get file name")?
                .into();
            let dest_file = format!("{}/{}", dest, file_name);
            snafu::ensure!(
                ctx.project_config_path.join(&dest_file).exists(),
                DestinationAlreadyExistsSnafu {
                    path: dest_file
                }
            );

            Ok((false, cleaned, dest_file, variable_path, file_name))
        })
        .try_collect()?;

    let get_system = || ctx.args.system.to_owned().context("could not get system");

    let mut new_links: Vec<_> = ctx
        .project
        .links
        .iter()
        .cloned()
        .map(|mut link| {
            let (_, _, dest_file, _, _) = match triples
                .iter_mut()
                .find(|(f, _, _, p, _)| p == &link.destination && !f)
            {
                None => return Ok(link),
                Some(x) => {
                    x.0 = true;
                    x
                }
            };
            let sys = get_system()?;
            link.src = link.src.insert_link(&sys, dest_file)?;

            Ok::<_, AddError>(link)
        })
        .try_collect()?;

    for (_, cleaned, dest_file, _, name) in &triples {
        info!("Linked {}", name);
        debug!(
            "cleaned = {}, dest_file = {}",
            cleaned.display(),
            ctx.project_config_path.join(dest_file).display()
        );
        move_link(cleaned, &ctx.project_config_path.join(dest_file)).await?;
    }

    for (_, _, dest_file, p, name) in triples.into_iter().filter(|x| x.0) {
        let source = SourceFile::Source {
            system: ctx.args.system.clone(),
            src: dest_file,
        };

        new_links.push(Link::new(name.to_string(), p, source));
    }

    let new_project = cascade! {
        ctx.project.clone();
        ..links = new_links;
    };
    Ok(new_project)
}

async fn move_link(original_locaction_cleaned: &Path, output_dest: &Path) -> Result<(),AddError> {
    if original_locaction_cleaned.is_dir() {
        recurse_copy(original_locaction_cleaned, output_dest).await?;
    } else {
        fs::copy(original_locaction_cleaned, output_dest).await?;
    }
    if fs::metadata(original_locaction_cleaned).await?.is_dir() {
        fs::remove_dir_all(original_locaction_cleaned).await?;
    } else {
        fs::remove_file(original_locaction_cleaned).await?;
    }
    debug!(
        "loc = {} \n dest = {}",
        original_locaction_cleaned.display(),
        output_dest.display()
    );

    fs::symlink(output_dest, original_locaction_cleaned).await?;
    Ok(())
}
