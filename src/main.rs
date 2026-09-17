#![allow(unreachable_code)]
mod cli;
mod cookbook;
mod dish;
mod error;
mod plan;
mod types;

use anyhow::Context;
use clap::Parser;
use cli::Cli;
use plan::Plan;
use std::fs;

use crate::{cookbook::CookBook, plan::WeekPlan};

fn main() -> anyhow::Result<()> {
    let Cli { plan, dish_root } = Cli::parse();

    let cookbook = CookBook::from_file(&dish_root)?;
    let week_plan = WeekPlan::from_file(&plan, &cookbook)?;
    let shopping_lists = week_plan.shopping_lists();

    // Generate concatenated markdown with numbered sections
    let mut output = String::new();
    for (i, mut list) in shopping_lists.into_iter().enumerate() {
        let section_number = i + 1;
        output.push_str(&format!("## Einkauf {}\n\n", section_number));
        output.push_str(&list.as_md_list());
        output.push_str("\n\n");
    }

    fs::write("./shopping-list.md", &output).context("Failed to write shopping-list.md")?;
    fs::write("./dishes.md", week_plan.dishes_as_markdown()).context("Failed to write dishes.md")?;

    Ok(())
}
