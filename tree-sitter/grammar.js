/**
 * @file Menu grammar for tree-sitter
 * @author Sebastian Klähn <info@sebastian-klaehn.de>
 * @license MIT
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

module.exports = grammar({
  name: "menu",

  extras: $ => [
    /[ \t]/, // ignore spaces but NOT newlines
  ],

  rules: {
    source_file: $ =>
      seq(
        optional($._line_break),
        $.persons_line,
        $._line_break,
        $.starttag_line,
        repeat(seq($._line_break, $.day_line)),
        optional($._line_break)
      ),

    // A line ending plus any number of following blank lines, as a single
    // token, so blank lines may be used freely to structure a plan.
    _line_break: _ => /\n([ \t]*\n)*/,

    // --------------------
    // Header
    // --------------------

    persons_line: $ =>
      seq("Personen:", field("count", $.integer)),

    starttag_line: $ =>
      seq("Starttag:", field("date", $.date)),

    integer: _ => /\d+/,

    date: _ => /\d{4}-\d{2}-\d{2}/,

    // --------------------
    // Day lines
    // --------------------

    day_line: $ =>
      seq(
        field("day", $.day_with_count),
        ":",
        optional($.menu)
      ),

    day_with_count: $ =>
      seq(
        field("name", $.day_name),
        optional(field("count", $.count))
      ),

    // Accept anything until colon or newline
    day_name: _ => /[^:\n(]+/,

    count: _ => /\(\d+\)/,

    // --------------------
    // Menu
    // --------------------

    menu: $ =>
      choice(
        $.rest_day,
        $.menu_items
      ),

    rest_day: _ => "Reste",

    menu_items: $ =>
      seq(
        $.menu_item,
        repeat(choice(seq(",", $.menu_item), $.menu_item))
      ),

    menu_item: $ =>
      choice(
        $.dish_with_count,
        $.shopping_marker
      ),

    shopping_marker: _ =>
      seq("⟨", /[^⟩]+/, "⟩"),

    // --------------------
    // Dishes
    // --------------------

    dish_with_count: $ =>
      seq(
        field("dish", $.dish),
        optional(field("count", $.count))
      ),

    dish: $ =>
      seq(
        "[[",
        field("name", /[^\]]+/),
        "]]"
      ),
  }
});
