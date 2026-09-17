use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};

pub(crate) struct CookBook {
    dishes: HashMap<String, PathBuf>,
}

/// Collect all dishes recursively from the given path.
fn collect_dishes(dishes: &mut Vec<PathBuf>, path: &Path) -> Result<()> {
    if path.is_dir() {
        let entries = fs::read_dir(path)
            .with_context(|| format!("Failed to read dish directory: {}", path.display()))?;
        for entry in entries {
            let entry = entry
                .with_context(|| format!("Failed to read entry in: {}", path.display()))?;
            collect_dishes(dishes, &entry.path())?;
        }
    } else {
        dishes.push(path.to_path_buf());
    }
    Ok(())
}

impl CookBook {
    pub(crate) fn from_file(path: &Path) -> Result<Self> {
        if !path.is_dir() {
            bail!(
                "Dish root `{}` is not a directory. \
                 Point --dish-root at the folder holding your recipe files.",
                path.display()
            );
        }

        let mut dish_paths = vec![];
        collect_dishes(&mut dish_paths, path)?;

        let dishes = dish_paths
            .iter()
            .filter_map(|path| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(|name| (name.to_string(), path.clone()))
            })
            .collect::<HashMap<_, _>>();

        Ok(Self { dishes })
    }

    /// Get a dish path by name.
    pub(crate) fn get(&self, name: &str) -> Option<&Path> {
        self.dishes.get(name).map(|p| p.as_path())
    }
}
