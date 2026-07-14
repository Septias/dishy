use std::{fs, iter::Sum, ops::Add, path::Path};

use anyhow::Context;
use tree_sitter::Parser;

use crate::{cookbook::CookBook, dish::Dish, types::IngredientList};

pub(crate) trait Plan {
    /// Generate a shopping list for all dishes.
    fn shopping_list(&self) -> IngredientList;
    fn from_file(path: &Path, cookbook: &CookBook) -> Self;
}

/// A single day with multiple dishes.
pub(crate) struct Day {
    /// List of dishes.
    pub(crate) dishes: Vec<Dish>,
    pub(crate) shopping_days: Vec<usize>,
}

impl Plan for Day {
    fn shopping_list(&self) -> IngredientList {
        self.dishes
            .iter()
            .map(|dish| IngredientList::from(dish.shopping_list()))
            .sum()
    }

    fn from_file(_path: &Path, _cookbook: &CookBook) -> Self {
        unimplemented!()
    }
}

impl Add for IngredientList {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self::Output {
        self.0.extend(rhs.0);
        self
    }
}

impl Sum for IngredientList {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(IngredientList::new(), |acc, list| acc + list)
    }
}

/// The week structure of a meal plan.
pub(crate) struct WeekPlan {
    /// Start date used for PDF export.
    pub(crate) _start: chrono::NaiveDate,
    /// Consecutive list of days.
    pub(crate) days: Vec<Day>,
}

impl WeekPlan {
    /// Generate markdown for all dishes with scaled quantities.
    pub(crate) fn dishes_as_markdown(&self) -> String {
        let mut output = String::new();

        for (day_idx, day) in self.days.iter().enumerate() {
            if !day.dishes.is_empty() {
                output.push_str(&format!("# Tag {}\n\n", day_idx + 1));

                for dish in &day.dishes {
                    output.push_str(&dish.as_markdown());
                    output.push_str("\n");
                }

                output.push_str("\n");
            }
        }

        output
    }

    /// Generate multiple shopping lists based on shopping markers across all days.
    /// Shopping lists span multiple days until a shopping marker is encountered.
    pub(crate) fn shopping_lists(&self) -> Vec<IngredientList> {
        // Flatten all dishes across all days and collect shopping marker positions
        let mut all_dishes = Vec::new();
        let mut marker_positions = Vec::new();
        let mut current_position = 0;

        for day in &self.days {
            // Add all dishes from this day
            all_dishes.extend(day.dishes.iter());

            // Adjust shopping marker positions to account for all previous dishes
            for &marker_idx in &day.shopping_days {
                marker_positions.push(current_position + marker_idx);
            }

            current_position += day.dishes.len();
        }

        // If no markers, return one list with all dishes
        if marker_positions.is_empty() {
            let list: IngredientList = all_dishes
                .iter()
                .map(|dish| IngredientList::from(dish.shopping_list()))
                .sum();
            return vec![list];
        }

        // Generate shopping lists for dishes between consecutive markers
        let mut lists = Vec::new();
        let mut start_idx = 0;

        for &marker_idx in &marker_positions {
            if marker_idx > start_idx {
                let list: IngredientList = all_dishes[start_idx..marker_idx]
                    .iter()
                    .map(|dish| IngredientList::from(dish.shopping_list()))
                    .sum();
                lists.push(list);
            }
            start_idx = marker_idx;
        }

        // Add remaining dishes after the last marker
        if start_idx < all_dishes.len() {
            let list: IngredientList = all_dishes[start_idx..]
                .iter()
                .map(|dish| IngredientList::from(dish.shopping_list()))
                .sum();
            lists.push(list);
        }

        lists
    }
}

impl Plan for WeekPlan {
    fn shopping_list(&self) -> IngredientList {
        self.days.iter().map(|day| day.shopping_list()).sum()
    }

    fn from_file(path: &Path, cookbook: &CookBook) -> Self {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read plan file: {}", path.display()))
            .unwrap();

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_menu::LANGUAGE.into())
            .expect("Error loading menu parser");

        let tree = parser.parse(&content, None).expect("Failed to parse");
        let root = tree.root_node();

        eprintln!("Root node kind: {}", root.kind());
        eprintln!("Root has error: {}", root.has_error());
        eprintln!("Tree: {}", root.to_sexp());

        let mut cursor = root.walk();

        let mut people = 1;
        let mut start_date = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let mut days = Vec::new();

        for child in root.children(&mut cursor) {
            eprintln!("Child kind: {}", child.kind());
            match child.kind() {
                "persons_line" => {
                    if let Some(count_node) = child.child_by_field_name("count") {
                        let count_str = content[count_node.byte_range()].trim();
                        people = count_str.parse().unwrap_or(1);
                        eprintln!("Parsed people: {}", people);
                    }
                }
                "starttag_line" => {
                    if let Some(date_node) = child.child_by_field_name("date") {
                        let date_str = content[date_node.byte_range()].trim();
                        start_date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                            .unwrap_or_else(|_| {
                                chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
                            });
                        eprintln!("Parsed date: {}", start_date);
                    }
                }
                "day_line" => {
                    let day = parse_day_line(&child, &content, cookbook, people);
                    eprintln!("Parsed day with {} dishes", day.dishes.len());
                    days.push(day);
                }
                _ => {}
            }
        }

        eprintln!("Total days: {}", days.len());
        eprintln!(
            "Total dishes across all days: {}",
            days.iter().map(|d| d.dishes.len()).sum::<usize>()
        );

        Self {
            _start: start_date,
            days,
        }
    }
}

fn parse_day_line(
    node: &tree_sitter::Node,
    content: &str,
    cookbook: &CookBook,
    default_people: usize,
) -> Day {
    let mut day_people = None;
    let mut dishes = Vec::new();
    let mut shopping_days = Vec::new();

    let mut cursor = node.walk();

    eprintln!("  Day line children:");
    for child in node.children(&mut cursor) {
        eprintln!("    - kind: {}", child.kind());
        match child.kind() {
            "day_with_count" => {
                if let Some(count_node) = child.child_by_field_name("count") {
                    let count_str = content[count_node.byte_range()].trim();
                    let count_str = count_str.trim_start_matches('(').trim_end_matches(')');
                    day_people = count_str.parse().ok();
                }
            }
            "menu" => {
                eprintln!("    Found menu node");
                parse_menu(
                    &child,
                    content,
                    cookbook,
                    &mut dishes,
                    &mut shopping_days,
                    default_people,
                    day_people,
                );
            }
            _ => {}
        }
    }

    Day {
        dishes,
        shopping_days,
    }
}

fn parse_menu(
    node: &tree_sitter::Node,
    content: &str,
    cookbook: &CookBook,
    dishes: &mut Vec<Dish>,
    shopping_days: &mut Vec<usize>,
    default_people: usize,
    day_people: Option<usize>,
) {
    let mut cursor = node.walk();

    eprintln!("      Menu children:");
    for child in node.children(&mut cursor) {
        eprintln!("        - kind: {}", child.kind());
        match child.kind() {
            "rest_day" => {
                return;
            }
            "menu_items" => {
                eprintln!("        Found menu_items");
                let mut items_cursor = child.walk();
                for item in child.children(&mut items_cursor) {
                    eprintln!("          Item kind: {}", item.kind());
                    if item.kind() == "menu_item" {
                        parse_menu_item(
                            &item,
                            content,
                            cookbook,
                            dishes,
                            shopping_days,
                            default_people,
                            day_people,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn parse_menu_item(
    node: &tree_sitter::Node,
    content: &str,
    cookbook: &CookBook,
    dishes: &mut Vec<Dish>,
    shopping_days: &mut Vec<usize>,
    default_people: usize,
    day_people: Option<usize>,
) {
    // The menu_item node directly contains either dish_with_count or shopping_marker
    // Get the first child which should be the actual content
    // let mut _cursor = node.walk();

    if let Some(child) = node.child(0) {
        eprintln!("          Menu item child kind: {}", child.kind());
        match child.kind() {
            "dish_with_count" => {
                if let Some(dish_node) = child.child_by_field_name("dish") {
                    eprintln!("            Found dish node: {}", dish_node.kind());

                    // Get the full dish text (e.g., "[[Dish Name]]")
                    let dish_text = content[dish_node.byte_range()].trim();
                    eprintln!("            Dish text: {}", dish_text);

                    // Strip the [[ and ]] brackets to get the dish name
                    if dish_text.starts_with("[[") && dish_text.ends_with("]]") {
                        let dish_name = &dish_text[2..dish_text.len() - 2];
                        eprintln!("            Dish name: {}", dish_name);

                        // Extract multiplier if present
                        let dish_people = child.child_by_field_name("count").map(|count_node| {
                            let count_str = content[count_node.byte_range()].trim();
                            // Remove parentheses from count
                            let count_str = count_str.trim_start_matches('(').trim_end_matches(')');
                            count_str.parse::<usize>().unwrap_or(1)
                        });

                        // Look up dish in cookbook
                        if let Some(dish_path) = cookbook.get(dish_name) {
                            eprintln!("            Found in cookbook: {:?}", dish_path);

                            // use the proper amount of people!
                            let people =
                                dish_people.unwrap_or(day_people.unwrap_or(default_people));
                            match Dish::from_file(dish_path, dish_name, people) {
                                Ok(dish) => {
                                    eprintln!(
                                        "            Loaded dish with {} ingredients",
                                        dish.ingredients.len()
                                    );
                                    dishes.push(dish);
                                }
                                Err(e) => {
                                    eprintln!("            Error loading dish: {}", e);
                                }
                            }
                        } else {
                            eprintln!("            NOT found in cookbook");
                        }
                    } else {
                        eprintln!("            Invalid dish format (missing brackets)");
                    }
                } else {
                    eprintln!("            No dish node found");
                }
            }
            "shopping_marker" => {
                shopping_days.push(dishes.len());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Ingredient;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    // Helper to create test ingredients
    fn make_ingredient(amount: f32, measure: &str, name: &str, dish: &str) -> Ingredient {
        Ingredient {
            amount,
            measure: measure.to_string(),
            name: name.to_string(),
            dish: dish.to_string(),
        }
    }

    // Helper to create a test dish file
    fn create_test_dish_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_ingredient_list_add() {
        let list1 = IngredientList::from(vec![
            make_ingredient(100.0, "g", "Butter", "Dish1"),
            make_ingredient(200.0, "ml", "Milch", "Dish1"),
        ]);

        let list2 = IngredientList::from(vec![make_ingredient(50.0, "g", "Zucker", "Dish2")]);

        let combined = list1 + list2;
        assert_eq!(combined.0.len(), 3);
    }

    #[test]
    fn test_ingredient_list_sum() {
        let lists = vec![
            IngredientList::from(vec![make_ingredient(100.0, "g", "Butter", "Dish1")]),
            IngredientList::from(vec![make_ingredient(200.0, "ml", "Milch", "Dish2")]),
            IngredientList::from(vec![make_ingredient(50.0, "g", "Zucker", "Dish3")]),
        ];

        let total: IngredientList = lists.into_iter().sum();
        assert_eq!(total.0.len(), 3);
    }

    #[test]
    fn test_ingredient_list_sum_empty() {
        let lists: Vec<IngredientList> = vec![];
        let total: IngredientList = lists.into_iter().sum();
        assert_eq!(total.0.len(), 0);
    }

    #[test]
    fn test_day_shopping_list_single_dish() {
        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(dish_content);
        let dish = Dish::from_file(file.path(), "Test Dish", 2).unwrap();

        let day = Day {
            dishes: vec![dish],
            shopping_days: vec![],
        };

        let shopping_list = day.shopping_list();
        assert_eq!(shopping_list.0.len(), 2);
        assert_eq!(shopping_list.0[0].amount, 100.0);
        assert_eq!(shopping_list.0[0].name, "Butter");
        assert_eq!(shopping_list.0[1].amount, 200.0);
        assert_eq!(shopping_list.0[1].name, "Milch");
    }

    #[test]
    fn test_day_shopping_list_multiple_dishes() {
        let dish1_content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch

## Zubereitung
1. Mix everything together.
"#;
        let dish2_content = r#"2 Personen

## Zutaten
- 50 g Zucker
- 3 Stück Eier

## Zubereitung
1. Mix everything together.
"#;
        let file1 = create_test_dish_file(dish1_content);
        let file2 = create_test_dish_file(dish2_content);

        let dish1 = Dish::from_file(file1.path(), "Dish1", 2).unwrap();
        let dish2 = Dish::from_file(file2.path(), "Dish2", 2).unwrap();

        let day = Day {
            dishes: vec![dish1, dish2],
            shopping_days: vec![],
        };

        let shopping_list = day.shopping_list();
        assert_eq!(shopping_list.0.len(), 4);
    }

    #[test]
    fn test_day_shopping_list_empty() {
        let day = Day {
            dishes: vec![],
            shopping_days: vec![],
        };
        let shopping_list = day.shopping_list();
        assert_eq!(shopping_list.0.len(), 0);
    }

    #[test]
    fn test_weekplan_shopping_list() {
        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let file = create_test_dish_file(dish_content);
        let dish1 = Dish::from_file(file.path(), "Dish1", 2).unwrap();
        let dish2 = Dish::from_file(file.path(), "Dish2", 2).unwrap();

        let day1 = Day {
            dishes: vec![dish1],
            shopping_days: vec![],
        };
        let day2 = Day {
            dishes: vec![dish2],
            shopping_days: vec![],
        };

        let weekplan = WeekPlan {
            _start: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            days: vec![day1, day2],
        };

        let shopping_list = weekplan.shopping_list();
        // Two dishes with same ingredient should result in 2 separate entries (before accumulation)
        assert_eq!(shopping_list.0.len(), 2);
    }

    #[test]
    fn test_weekplan_from_file_parses_persons() {
        let menu_content = r#"Personen: 4
Starttag: 2026-03-01
Montag: [[Test Dish]]
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let _dish_file = create_test_dish_file(dish_content);

        // Create a temp dir for cookbook
        let temp_dir = TempDir::new().unwrap();
        let dish_path = temp_dir.path().join("Test Dish.txt");
        std::fs::write(&dish_path, dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());

        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        // Should have parsed the file successfully
        assert_eq!(weekplan.days.len(), 1);
        assert_eq!(weekplan.days[0].dishes.len(), 1);
        // Should have scaled to 4 people (2x the recipe)
        let ingredients = weekplan.days[0].dishes[0].shopping_list();
        assert_eq!(ingredients[0].amount, 200.0); // 100g * 2
    }

    #[test]
    fn test_weekplan_from_file_parses_start_date() {
        let menu_content = r#"Personen: 2
Starttag: 2026-12-25
Montag: [[Test Dish]]
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let temp_dir = TempDir::new().unwrap();
        let dish_path = temp_dir.path().join("Test Dish.txt");
        std::fs::write(&dish_path, dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());
        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        assert_eq!(
            weekplan._start,
            chrono::NaiveDate::from_ymd_opt(2026, 12, 25).unwrap()
        );
    }

    #[test]
    fn test_weekplan_from_file_multiple_dishes_per_day() {
        let menu_content = r#"Personen: 2
Starttag: 2026-01-01
Montag: [[Dish1]], [[Dish2]]
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Dish1.txt"), dish_content).unwrap();
        std::fs::write(temp_dir.path().join("Dish2.txt"), dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());
        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        assert_eq!(weekplan.days.len(), 1);
        assert_eq!(weekplan.days[0].dishes.len(), 2);
    }

    #[test]
    fn test_weekplan_from_file_multiple_days() {
        let menu_content = r#"Personen: 2
Starttag: 2026-01-01
Montag: [[Dish1]]
Dienstag: [[Dish2]]
Mittwoch: [[Dish3]]
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Dish1.txt"), dish_content).unwrap();
        std::fs::write(temp_dir.path().join("Dish2.txt"), dish_content).unwrap();
        std::fs::write(temp_dir.path().join("Dish3.txt"), dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());
        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        assert_eq!(weekplan.days.len(), 3);
        assert_eq!(weekplan.days[0].dishes.len(), 1);
        assert_eq!(weekplan.days[1].dishes.len(), 1);
        assert_eq!(weekplan.days[2].dishes.len(), 1);
    }

    #[test]
    fn test_weekplan_from_file_rest_day() {
        let menu_content = r#"Personen: 2
Starttag: 2026-01-01
Montag: [[Dish1]]
Dienstag: Reste
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Dish1.txt"), dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());
        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        assert_eq!(weekplan.days.len(), 2);
        assert_eq!(weekplan.days[0].dishes.len(), 1);
        assert_eq!(weekplan.days[1].dishes.len(), 0); // Rest day = no dishes
    }

    #[test]
    fn test_weekplan_from_file_dish_with_count() {
        let menu_content = r#"Personen: 2
Starttag: 2026-01-01
Montag: [[Dish1]](4)
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Dish1.txt"), dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());
        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        assert_eq!(weekplan.days.len(), 1);
        assert_eq!(weekplan.days[0].dishes.len(), 1);
        // Should be scaled to 4 people (dish count override)
        let ingredients = weekplan.days[0].dishes[0].shopping_list();
        assert_eq!(ingredients[0].amount, 200.0); // 100g * 2
    }

    #[test]
    fn test_weekplan_from_file_daycount() {
        let menu_content = r#"Personen: 2
Starttag: 2026-01-01
Montag(4): [[Dish1]]
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Dish1.txt"), dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());
        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        assert_eq!(weekplan.days.len(), 1);
        assert_eq!(weekplan.days[0].dishes.len(), 1);
        // Should be scaled to 4 people (dish count override)
        let ingredients = weekplan.days[0].dishes[0].shopping_list();
        assert_eq!(ingredients[0].amount, 200.0); // 100g * 2
    }

    #[test]
    fn test_weekplan_from_file_daycount_complex() {
        let menu_content = r#"Personen: 2
Starttag: 2026-01-01
Montag(4): [[Dish1]](2)
"#;
        let menu_file = create_test_dish_file(menu_content);

        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("Dish1.txt"), dish_content).unwrap();

        let cookbook = CookBook::from_file(temp_dir.path());
        let weekplan = WeekPlan::from_file(menu_file.path(), &cookbook);

        assert_eq!(weekplan.days.len(), 1);
        assert_eq!(weekplan.days[0].dishes.len(), 1);
        let ingredients = weekplan.days[0].dishes[0].shopping_list();
        assert_eq!(ingredients[0].amount, 100.0);
    }

    #[test]
    fn test_weekplan_shopping_lists_spans_multiple_days() {
        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let file1 = create_test_dish_file(dish_content);
        let file2 = create_test_dish_file(dish_content);
        let file3 = create_test_dish_file(dish_content);
        let file4 = create_test_dish_file(dish_content);

        let dish1 = Dish::from_file(file1.path(), "Dish1", 2).unwrap();
        let dish2 = Dish::from_file(file2.path(), "Dish2", 2).unwrap();
        let dish3 = Dish::from_file(file3.path(), "Dish3", 2).unwrap();
        let dish4 = Dish::from_file(file4.path(), "Dish4", 2).unwrap();

        // Day 1: Dish1, Dish2
        // Day 2: Dish3, Shopping Marker, Dish4
        let day1 = Day {
            dishes: vec![dish1, dish2],
            shopping_days: vec![],
        };
        let day2 = Day {
            dishes: vec![dish3, dish4],
            shopping_days: vec![1], // Marker after Dish3
        };

        let weekplan = WeekPlan {
            _start: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            days: vec![day1, day2],
        };

        let lists = weekplan.shopping_lists();
        assert_eq!(lists.len(), 2);

        // First list should contain Dish1, Dish2, Dish3 (spans day 1 and part of day 2)
        assert_eq!(lists[0].0.len(), 3);

        // Second list should contain Dish4 (after the marker)
        assert_eq!(lists[1].0.len(), 1);
    }

    #[test]
    fn test_weekplan_shopping_lists_no_markers() {
        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter

## Zubereitung
1. Mix everything together.
"#;
        let file1 = create_test_dish_file(dish_content);
        let file2 = create_test_dish_file(dish_content);

        let dish1 = Dish::from_file(file1.path(), "Dish1", 2).unwrap();
        let dish2 = Dish::from_file(file2.path(), "Dish2", 2).unwrap();

        let day1 = Day {
            dishes: vec![dish1],
            shopping_days: vec![],
        };
        let day2 = Day {
            dishes: vec![dish2],
            shopping_days: vec![],
        };

        let weekplan = WeekPlan {
            _start: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            days: vec![day1, day2],
        };

        let lists = weekplan.shopping_lists();
        assert_eq!(lists.len(), 1);
        // All dishes in one list
        assert_eq!(lists[0].0.len(), 2);
    }

    #[test]
    fn test_weekplan_dishes_as_markdown() {
        let dish_content = r#"2 Personen

## Zutaten
- 100 g Butter
- 200 ml Milch

## Zubereitung
1. Mix everything together.
"#;
        let file1 = create_test_dish_file(dish_content);
        let file2 = create_test_dish_file(dish_content);

        let dish1 = Dish::from_file(file1.path(), "Pasta", 4).unwrap();
        let dish2 = Dish::from_file(file2.path(), "Salad", 2).unwrap();

        let day1 = Day {
            dishes: vec![dish1, dish2],
            shopping_days: vec![],
        };

        let weekplan = WeekPlan {
            _start: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            days: vec![day1],
        };

        let markdown = weekplan.dishes_as_markdown();

        assert!(markdown.contains("# Tag 1"));
        assert!(markdown.contains("## Pasta (4 Personen)"));
        assert!(markdown.contains("## Salad (2 Personen)"));
        assert!(markdown.contains("- 200.0 g Butter")); // Pasta scaled to 4
        assert!(markdown.contains("- 100.0 g Butter")); // Salad at 2
        assert!(markdown.contains("## Zubereitung"));
        assert!(markdown.contains("1. Mix everything together."));
    }
}
