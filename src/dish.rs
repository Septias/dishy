use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use tree_sitter::Parser;

use crate::types::Ingredient;
use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum DishPlanError {
    #[error("The plan does not exist at the given location.")]
    DishError,
}

/// A single dish.
#[derive(Debug)]
pub(crate) struct Dish {
    /// Name of the dish.
    pub(crate) name: String,
    /// The amount of people to feed.
    pub(crate) people: Option<usize>,
    /// For how many people the recipe is scaled.
    pub(crate) recepie_people: usize,
    /// List of recipe ingredients.
    pub(crate) ingredients: Vec<Ingredient>,
    /// Other text
    pub(crate) blocks: Vec<String>,
    /// Path
    pub(crate) path: PathBuf,
}

impl Dish {
    pub(crate) fn from_file(path: &Path, dish_name: &str, people: usize) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read dish file: {}", path.display()))?;

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_dish::LANGUAGE.into())
            .context("Error loading dish parser")?;

        let tree = parser
            .parse(&content, None)
            .context("Failed to parse dish file")?;
        let root = tree.root_node();

        if root.has_error() {
            panic!("Parse error in dish file: {}", path.display());
        }

        let mut cursor = root.walk();
        let mut recipe_people = 1;
        let mut ingredients = Vec::new();
        let mut preparation_text = String::new();

        for child in root.children(&mut cursor) {
            match child.kind() {
                "persons_line" => {
                    if let Some(count_node) = child.child_by_field_name("count") {
                        let count_str = content[count_node.byte_range()].trim();
                        recipe_people = count_str.parse().unwrap_or(1);
                    }
                }
                "ingredients_section" => {
                    parse_ingredients_section(&child, &content, dish_name, &mut ingredients);
                }
                "preparation_section" => {
                    // Extract the full text of the preparation section
                    let section_text = content[child.byte_range()].trim();
                    preparation_text = section_text.to_string();
                }
                _ => {}
            }
        }

        Ok(Self {
            name: dish_name.to_string(),
            people: Some(people),
            recepie_people: recipe_people,
            ingredients,
            blocks: if preparation_text.is_empty() {
                vec![]
            } else {
                vec![preparation_text]
            },
            path: path.to_path_buf(),
        })
    }

    pub(crate) fn shopping_list(&self) -> Vec<Ingredient> {
        let base = self.recepie_people.max(1) as f32;
        let target = self.people.unwrap_or(self.recepie_people) as f32;
        let scale = target / base;

        self.ingredients
            .iter()
            .map(|ing| Ingredient {
                amount: ing.amount * scale,
                measure: ing.measure.clone(),
                name: ing.name.clone(),
                dish: ing.dish.clone(),
            })
            .collect()
    }

    /// Generate markdown for the dish with scaled quantities.
    pub(crate) fn as_markdown(&self) -> String {
        let target_people = self.people.unwrap_or(self.recepie_people);
        println!("{target_people}");
        let scaled_ingredients = self.shopping_list();

        let mut output = String::new();
        output.push_str(&format!(
            "## {} ({} Personen)\n\n",
            self.name, target_people
        ));
        output.push_str("### Zutaten\n");

        for ingredient in scaled_ingredients {
            let amount_str = if ingredient.measure.is_empty() {
                format!("{:.1}", ingredient.amount)
            } else {
                format!("{:.1} {}", ingredient.amount, ingredient.measure)
            };
            output.push_str(&format!("- {} {}\n", amount_str, ingredient.name));
        }

        // Add preparation section if it exists
        if !self.blocks.is_empty() {
            output.push('\n');
            output.push_str("#");
            for block in &self.blocks {
                output.push_str(block);
                output.push('\n');
            }
        }

        output
    }
}

fn parse_ingredients_section(
    node: &tree_sitter::Node,
    content: &str,
    dish_name: &str,
    ingredients: &mut Vec<Ingredient>,
) {
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "ingredient_line" {
            if let Some(ingredient) = parse_ingredient_node(&child, content, dish_name) {
                ingredients.push(ingredient);
            }
        }
    }
}

fn parse_ingredient_node(
    node: &tree_sitter::Node,
    content: &str,
    dish_name: &str,
) -> Option<Ingredient> {
    let name_node = node.child_by_field_name("name")?;
    let name = content[name_node.byte_range()].trim().to_string();

    let amount = if let Some(quantity_node) = node.child_by_field_name("quantity") {
        let quantity_str = content[quantity_node.byte_range()].trim();
        quantity_str.parse::<f32>().ok()?
    } else {
        1.0
    };

    let unit = if let Some(unit_node) = node.child_by_field_name("unit") {
        content[unit_node.byte_range()].trim().to_string()
    } else {
        String::new()
    };

    Some(Ingredient {
        amount,
        measure: unit,
        name,
        dish: dish_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_dish_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_parse_simple_dish() {
        let content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch
- 3 Stück Eier

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Test Dish", 2).unwrap();

        assert_eq!(dish.recepie_people, 2);
        assert_eq!(dish.people, Some(2));
        assert_eq!(dish.ingredients.len(), 3);

        // Check first ingredient
        assert_eq!(dish.ingredients[0].amount, 100.0);
        assert_eq!(dish.ingredients[0].measure, "g");
        assert_eq!(dish.ingredients[0].name, "Butter");
        assert_eq!(dish.ingredients[0].dish, "Test Dish");

        // Check second ingredient
        assert_eq!(dish.ingredients[1].amount, 200.0);
        assert_eq!(dish.ingredients[1].measure, "ml");
        assert_eq!(dish.ingredients[1].name, "Milch");

        // Check third ingredient
        assert_eq!(dish.ingredients[2].amount, 3.0);
        assert_eq!(dish.ingredients[2].measure, "Stück");
        assert_eq!(dish.ingredients[2].name, "Eier");
    }

    #[test]
    fn test_parse_with_float_quantity() {
        let content = r#"2 Personen

## Zutaten
- 0.5 TL Salz
- 1.5 EL Zucker

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Test Dish", 2).unwrap();

        assert_eq!(dish.ingredients.len(), 2);

        // Check float quantities
        assert_eq!(dish.ingredients[0].amount, 0.5);
        assert_eq!(dish.ingredients[0].measure, "TL");
        assert_eq!(dish.ingredients[0].name, "Salz");

        assert_eq!(dish.ingredients[1].amount, 1.5);
        assert_eq!(dish.ingredients[1].measure, "EL");
        assert_eq!(dish.ingredients[1].name, "Zucker");
    }

    #[test]
    fn test_parse_auberginen_corpus() {
        let content = r#"2 Personen

## Zutaten
- 4 EL Sesamöl
- 2 EL Miso Paste
- 2 TL Sojasauce
- 0.5 TL Sesamöl
- 1 TL Reisessig
- 1 TL Kokosblütenzucker
- 1 TL heißes Wasser

## Zubereitung
1. Miso Paste, Sojasauce, Sesamöl und 1 TL Reisessig und Kokosblütenzucker mit 1 TL heißem Wasser aufmischen.
2. Aubergine in der Mitte durchschneiden, längs halbieren und von innen gekreuzt einschneiden. Mit Sesamöl und Salz beträufeln. bei mittlerer Hitze für 5 Minuten anbraten. Aus der Pfanne nehmen und das Innere reichlich mit Miso Marinade bestreichen. Weitere 5 Minuten braten
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Auberginen", 2).unwrap();

        assert_eq!(dish.recepie_people, 2);
        assert_eq!(dish.people, Some(2));
        assert_eq!(dish.ingredients.len(), 7);

        // Spot check some ingredients
        assert_eq!(dish.ingredients[0].amount, 4.0);
        assert_eq!(dish.ingredients[0].measure, "EL");
        assert_eq!(dish.ingredients[0].name, "Sesamöl");

        assert_eq!(dish.ingredients[3].amount, 0.5);
        assert_eq!(dish.ingredients[3].measure, "TL");
        assert_eq!(dish.ingredients[3].name, "Sesamöl");
    }

    #[test]
    fn test_parse_invalid_format_fails() {
        let content = r#"This is not a valid dish file
Just some random text
"#;
        let file = create_test_dish_file(content);
        let result = Dish::from_file(file.path(), "Invalid Dish", 2);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Parse error"));
    }

    #[test]
    fn test_parse_missing_persons_line() {
        let content = r#"
## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let result = Dish::from_file(file.path(), "Test Dish", 2);

        // Should fail to parse without persons line
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_with_tabs() {
        let content = "4 Portionen\n\n## Zutaten\n- 5\tEL\tRum, Cognac oder Wasser\n- 100\tg\tButter\n\n## Zubereitung\n1. Mix everything together.\n";
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Test Dish", 4).unwrap();

        assert_eq!(dish.ingredients.len(), 2);

        // Check tab-separated ingredient
        assert_eq!(dish.ingredients[0].amount, 5.0);
        assert_eq!(dish.ingredients[0].measure, "EL");
        assert_eq!(dish.ingredients[0].name, "Rum, Cognac oder Wasser");

        assert_eq!(dish.ingredients[1].amount, 100.0);
        assert_eq!(dish.ingredients[1].measure, "g");
        assert_eq!(dish.ingredients[1].name, "Butter");
    }

    #[test]
    fn test_shopping_list_scales_up() {
        let content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch
- 3 Stück Eier

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Test Dish", 4).unwrap();

        let items = dish.shopping_list();
        assert_eq!(items.len(), 3);
        assert!((items[0].amount - 200.0).abs() < 1e-6);
        assert_eq!(items[0].measure, "g");
        assert_eq!(items[0].name, "Butter");

        assert!((items[1].amount - 400.0).abs() < 1e-6);
        assert_eq!(items[1].measure, "ml");
        assert_eq!(items[1].name, "Milch");

        assert!((items[2].amount - 6.0).abs() < 1e-6);
        assert_eq!(items[2].measure, "Stück");
        assert_eq!(items[2].name, "Eier");
    }

    #[test]
    fn test_shopping_list_scales_down() {
        let content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch
- 3 Stück Eier

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Test Dish", 1).unwrap();

        let items = dish.shopping_list();
        assert_eq!(items.len(), 3);
        assert!((items[0].amount - 50.0).abs() < 1e-6);
        assert_eq!(items[0].measure, "g");
        assert_eq!(items[0].name, "Butter");

        assert!((items[1].amount - 100.0).abs() < 1e-6);
        assert_eq!(items[1].measure, "ml");
        assert_eq!(items[1].name, "Milch");

        assert!((items[2].amount - 1.5).abs() < 1e-6);
        assert_eq!(items[2].measure, "Stück");
        assert_eq!(items[2].name, "Eier");
    }

    #[test]
    fn test_shopping_list_no_people_defaults_to_recipe_scale() {
        let content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let mut dish = Dish::from_file(file.path(), "Test Dish", 2).unwrap();
        // simulate "no requested people" -> should scale by 1.0 (use recipe_people)
        dish.people = None;

        let items = dish.shopping_list();
        assert_eq!(items.len(), 2);
        assert!((items[0].amount - 100.0).abs() < 1e-6);
        assert_eq!(items[0].measure, "g");
        assert_eq!(items[0].name, "Butter");

        assert!((items[1].amount - 200.0).abs() < 1e-6);
        assert_eq!(items[1].measure, "ml");
        assert_eq!(items[1].name, "Milch");
    }

    #[test]
    fn test_parse_ingredients_with_defaults() {
        let content = r#"2 Personen

## Zutaten
- 100 g Butter
- 2 Eggs
- Salt

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Test Dish", 2).unwrap();

        assert_eq!(dish.ingredients.len(), 3);

        // First ingredient: has amount and measure
        assert_eq!(dish.ingredients[0].amount, 100.0);
        assert_eq!(dish.ingredients[0].measure, "g");
        assert_eq!(dish.ingredients[0].name, "Butter");
        assert_eq!(dish.ingredients[0].dish, "Test Dish");

        // Second ingredient: has amount but no measure (defaults to empty string)
        assert_eq!(dish.ingredients[1].amount, 2.0);
        assert_eq!(dish.ingredients[1].measure, "");
        assert_eq!(dish.ingredients[1].name, "Eggs");
        assert_eq!(dish.ingredients[1].dish, "Test Dish");

        // Third ingredient: no amount or measure (defaults to 1.0 and empty string)
        assert_eq!(dish.ingredients[2].amount, 1.0);
        assert_eq!(dish.ingredients[2].measure, "");
        assert_eq!(dish.ingredients[2].name, "Salt");
        assert_eq!(dish.ingredients[2].dish, "Test Dish");
    }

    #[test]
    fn test_as_markdown_with_scaling() {
        let content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch
- 3 Stück Eier

## Zubereitung
1. Mix everything together.
2. Bake for 30 minutes.
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Test Dish", 4).unwrap();

        let markdown = dish.as_markdown();

        assert!(markdown.contains("## Test Dish (4 Personen)"));
        assert!(markdown.contains("### Zutaten"));
        assert!(markdown.contains("- 200.0 g Butter"));
        assert!(markdown.contains("- 400.0 ml Milch"));
        assert!(markdown.contains("- 6.0 Stück Eier"));
        assert!(markdown.contains("## Zubereitung"));
        assert!(markdown.contains("1. Mix everything together."));
        assert!(markdown.contains("2. Bake for 30 minutes."));
    }

    #[test]
    fn test_as_markdown_no_scaling() {
        let content = r#"2 Personen

## Zutaten
- 100 g Butter
- 2 Eggs

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(content);
        let dish = Dish::from_file(file.path(), "Simple Dish", 2).unwrap();

        let markdown = dish.as_markdown();

        assert!(markdown.contains("## Simple Dish (2 Personen)"));
        assert!(markdown.contains("- 100.0 g Butter"));
        assert!(markdown.contains("- 2.0 Eggs"));
    }
}
