module.exports = grammar({
  name: "dish",

  extras: _ => [/[\t \n]+/],

  rules: {
    source_file: $ =>
      seq(
        $.persons_line,
        repeat($.preamble_line),
        $.ingredients_section,
        optional($.preparation_section),
      ),

    persons_line: $ =>
      seq(
        field("count", $.integer),
        /[\t ]+/,
        choice("Personen", "Portionen")
      ),

    ingredients_section: $ => seq("## Zutaten", repeat1($.ingredient_line)),
    preparation_section: $ => seq("## Zubereitung", repeat($.text)),

    ingredient_line: $ =>
      choice(
         seq("-", field("quantity", $.quantity), field("unit", $.unit), field("name", $.ingredient_name)),
         seq("-", field("quantity", $.quantity), field("name", $.ingredient_name)),
         seq("-", field("name", $.ingredient_name)),
         $.preamble_line,
      ),


    // Tokens
    quantity: $ => choice($.float, $.integer),
    integer: _ => token(prec(2,/\d+/)),
    float: _ => token(prec(2,/\d+[\.,]\d+/)),
    unit: _ => token(prec(3, choice("Dosen", "Dose", "g", "G", "mg", "MG", "kg", "KG", "el", "EL", "tl", "TL", "l", "L", "ml","ML", "Liter", "stk", "Stk", "Scheiben", "scheiben", "scheibe", "Pr.", "Stück", "Packung", "Packungen", "Pkg.", "Prise", "Stiele", "Bund", "Messerspitze", "Msp", "Glas", "glas"))),
    text: _ => /[^\n\r]+/,
    ingredient_name: _ => /[^\n\r-]+/,
    preamble_line: _ => /[^#\-\n\r][^\n\r]*/,
  }
});
