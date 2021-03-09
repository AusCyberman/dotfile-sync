use anyhow::Result;
use std::{fs, fmt::Error, fs::File, path::PathBuf};
use std::io::{Read,Write};
use std::env;
use serde::{Deserialize,Serialize};
use std::collections::HashMap;
use clap::{Arg, App, SubCommand };
use directories::{UserDirs, ProjectDirs};


type System = String;

fn check_path(_path : String) -> Result<PathBuf> {
       let path : PathBuf = PathBuf::from(&_path);

       if path.exists() { anyhow::bail!("File does not exist: {}", _path);}
       if path.metadata()
           .map(|x|x.permissions().readonly())
           .unwrap_or(false) {
            anyhow::bail!("Invalid permissions for file: {}",_path);
            }


        Ok(path)
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(untagged)]
enum Destination {
        DefaultDest(PathBuf),
        DynamicDestination(PathBuf,HashMap<System,PathBuf>)
}

impl Destination {
   pub fn new(dest : String) -> Result<Destination>{
        Ok(
            Destination::DefaultDest(check_path(dest)?)
            )
   }
   pub fn new_dyn(default : String, system_map : HashMap<System,String>) -> Result<Destination>{
       let mut new_map : HashMap<System,PathBuf> = HashMap::new();
            for (key,elem) in system_map{
                new_map.insert(key, check_path(elem)?);
            }
        Ok(
                Destination::DynamicDestination(check_path(default)?,new_map)
          )
   }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct Link {
    name : String,
    src : PathBuf,
    destination : Destination
}

impl Link {
    pub fn new(name : String, src : String, destination : Destination) -> Result<Link>{
       Ok(
           Link {
           name, src : check_path(src)?, destination

        })
}}

#[derive(Deserialize, Serialize, Debug)]
struct Config {
    systems : Vec<System>,
    links : Vec<Link>
}



fn main() -> Result<()>{

    let config_file = if let Some(proj_dir) = ProjectDirs::from("com", "AusCyber", "SymSync"){
        let conf_dir = proj_dir.config_dir().to_path_buf();

        if !conf_dir.exists(){
           fs::create_dir_all(&conf_dir)?;
        }
        let config_file_loc = { let mut x = conf_dir.clone(); x.push(".links.toml"); x};

        let f = fs::File::create(config_file_loc)?;

    };
    let matches = App::new("SymSync");


    Ok(())
}

