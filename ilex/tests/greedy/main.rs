
use ilex::rule::*;
use ilex::Context;
use ilex::Lexeme;

#[gilded::test("tests/greedy/*.txt")]
fn greedy(test: &mut gilded::Test) {
  // This test verifies that lexing is greedy in *most* cases.

  #[ilex::spec]
  struct Greedy {
    #[rule(Quoted::with(Bracket::rust_style(
      "#%",
      ("poisonous", "["),
      ("]", ">"),
    )))]
    rust_like: Lexeme<Quoted>, 
  
    #[rule(Quoted::with(Bracket::cxx_style(
      Ident::new(),
      ("R\"", "("),
      (")", "\""),
    )))]
    cpp_like: Lexeme<Quoted>, 
  
    #[rule("[", "]")]
    array: Lexeme<Bracket>,
  
    poison: Lexeme<Keyword>,
  
    #[rule(Ident::new())]
    ident: Lexeme<Ident>,
  }

  let ctx = Context::new();
  let report = ctx.new_report();
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Greedy::get().spec(), &report) {
    Ok(stream) => {
      test.output("tokens.yaml", stream.summary());
      test.output("stderr", "".into());
    }
    Err(fatal) => {
      test.output("tokens.yaml", "".into());
      test.output("stderr", format!("{fatal:?}"));
    }
  }
}
