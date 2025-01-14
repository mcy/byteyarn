use ilex::rule::*;
use ilex::Context;
use ilex::Lexeme;

#[ilex::spec]
struct Llvm {
  #[rule(";")]
  comment: Lexeme<Comment>,

  #[rule('(', ')')]
  parens: Lexeme<Bracket>,
  #[rule('[', ']')]
  brackets: Lexeme<Bracket>,
  #[rule('<', '>')]
  vector: Lexeme<Bracket>,
  #[rule('{', '}')]
  braces: Lexeme<Bracket>,
  #[rule("<{", "}>")]
  packed: Lexeme<Bracket>,
  #[rule("!{", "}")]
  meta: Lexeme<Bracket>,

  #[rule(',')]
  comma: Lexeme<Keyword>,
  #[rule('=')]
  equal: Lexeme<Keyword>,
  #[rule('*')]
  star: Lexeme<Keyword>,
  #[rule('x')]
  times: Lexeme<Keyword>,

  br: Lexeme<Keyword>,
  call: Lexeme<Keyword>,
  icmp: Lexeme<Keyword>,
  #[rule("eq")]
  icmp_eq: Lexeme<Keyword>,
  ret: Lexeme<Keyword>,
  unreachable: Lexeme<Keyword>,

  constant: Lexeme<Keyword>,
  declare: Lexeme<Keyword>,
  define: Lexeme<Keyword>,
  global: Lexeme<Keyword>,

  label: Lexeme<Keyword>,
  null: Lexeme<Keyword>,
  ptr: Lexeme<Keyword>,
  #[rule(Digital::new(10).prefix("i"))]
  int: Lexeme<Digital>,
  void: Lexeme<Keyword>,

  private: Lexeme<Keyword>,
  unnamed_addr: Lexeme<Keyword>,
  nocapture: Lexeme<Keyword>,
  nounwind: Lexeme<Keyword>,

  #[named]
  #[rule(Quoted::new('"')
    .fixed_length_escape(r"\", 2)
    .prefixes(["", "c"]))]
  string: Lexeme<Quoted>,

  #[named("identifier")]
  #[rule(Ident::new()
    .ascii_only()
    .extra_starts(".0123456789".chars())
    .suffix(":"))]
  label_ident: Lexeme<Ident>,

  #[named("identifier")]
  #[rule(Ident::new()
    .ascii_only()
    .extra_starts(".0123456789".chars())
    .prefixes(["!", "@", "%"]))]
  bare: Lexeme<Ident>,

  #[named("quoted identifier")]
  #[rule(Quoted::new('"')
    .fixed_length_escape(r"\", 2)
    .prefixes(["!", "@", "%"]))]
  quoted: Lexeme<Quoted>,

  #[named = "number"]
  #[rule(Digital::new(10)
    .minus()
    .point_limit(0..2)
    .exponents(["e", "E"], Digits::new(10).plus().minus()))]
  dec: Lexeme<Digital>,

  #[named = "number"]
  #[rule(Digital::new(16).minus().prefix("0x"))]
  hex: Lexeme<Digital>,
}

#[gilded::test("tests/llvm/*.ll")]
fn llvm(test: &mut gilded::Test) {
  let ctx = Context::new();
  let report = ctx.new_report();
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Llvm::get().spec(), &report) {
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
