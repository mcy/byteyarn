use ilex::report::Options;
use ilex::rule::*;
use ilex::Context;
use ilex::Lexeme;

#[gilded::test("tests/ui/ambiguous/*.txt")]
fn ambiguous(test: &mut gilded::Test) {
  #[ilex::spec]
  struct Spec {
    #[rule("null")]
    kw: Lexeme<Keyword>,
    #[rule("-null")]
    kw2: Lexeme<Keyword>,
    #[rule(")null")]
    kw3: Lexeme<Keyword>,

    #[rule(Comment::nesting(Bracket::rust_style(
        "/",
        ("-", ""),
        ("", "-"),
      )))]
    cm: Lexeme<Comment>,
    #[rule(Comment::nesting(Bracket::cxx_style(
        Ident::new().min_len(1),
        ("--", ""),
        ("", ""),
      )))]
    cm2: Lexeme<Comment>,
    #[rule(Bracket::cxx_style(
        Ident::new(),
        ("$", "["),
        ("]", ""),
      ))]
    br: Lexeme<Bracket>,
    #[rule(Ident::new()
      .prefix("/")
      .suffixes(["", "%q", "/"]))]
    id: Lexeme<Ident>,
    #[rule(Digital::new(10)
        .prefixes(["", "%"])
        .suffixes(["", "%", "q", "/"]))]
    nm: Lexeme<Digital>,
    #[rule(Quoted::new("'")
        .prefixes(["%", "q"])
        .suffixes(["", "%", "q"]))]
    st: Lexeme<Quoted>,
    #[rule(Quoted::with(Bracket::cxx_style(
          Ident::new(),
          ("q", "("),
          (")", ""),
        )))]
    st2: Lexeme<Quoted>,
  }

  let ctx = Context::new();
  let report =
    ctx.new_report_with(Options { color: false, ..Default::default() });
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Spec::get().spec(), &report) {
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

#[gilded::test("tests/ui/digital/*.txt")]
fn digital(test: &mut gilded::Test) {
  #[ilex::spec]
  struct Spec {
    #[rule(Digital::new(16).prefix("0x"))]
    m1: Lexeme<Digital>,
    #[rule(Digital::new(8).prefix("0o"))]
    m2: Lexeme<Digital>,

    #[rule( Digital::new(10)
      .point_limit(2..3)
      .point('/')
      .exponent("e", Digits::new(10).point_limit(1..2))
      .separator_with("_",
        SeparatorCornerCases {
          prefix: true,
          suffix: true,
          around_point: true,
          around_exp: true,
        }))]
    m0: Lexeme<Digital>,
    #[rule(Digital::new(10)
          .prefix("all_ok@")
          .point_limit(0..3)
          .exponent("e", Digits::new(10).point_limit(0..3))
          .separator_with("_",
            SeparatorCornerCases {
              prefix: true,
              suffix: true,
              around_point: true,
              around_exp: true,
            }))]
    n0: Lexeme<Digital>,
    #[rule( Digital::new(10)
        .prefix("no_prefix@")
        .point_limit(0..3)
        .exponent("e", Digits::new(10).point_limit(0..3))
        .separator_with("_",
          SeparatorCornerCases {
            prefix: false,
            suffix: true,
            around_point: true,
            around_exp: true,
          }))]
    n1: Lexeme<Digital>,
    #[rule(Digital::new(10)
        .prefix("no_suffix@")
        .point_limit(0..3)
        .exponent("e", Digits::new(10).point_limit(0..3))
        .separator_with("_",
          SeparatorCornerCases {
            prefix: true,
            suffix: false,
            around_point: true,
            around_exp: true,
          }))]
    n2: Lexeme<Digital>,
    #[rule( Digital::new(10)
        .prefix("no_point@")
        .point_limit(0..3)
        .exponent("e", Digits::new(10).point_limit(0..3))
        .separator_with("_",
          SeparatorCornerCases {
            prefix: true,
            suffix: true,
            around_point: false,
            around_exp: true,
          }))]
    n3: Lexeme<Digital>,
    #[rule(Digital::new(10)
        .prefix("no_exp@")
        .point_limit(0..3)
        .exponent("e", Digits::new(10).point_limit(0..3))
        .separator_with("_",
          SeparatorCornerCases {
            prefix: true,
            suffix: true,
            around_point: true,
            around_exp: false,
          }))]
    n4: Lexeme<Digital>,
  }

  let ctx = Context::new();
  let report =
    ctx.new_report_with(Options { color: false, ..Default::default() });
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Spec::get().spec(), &report) {
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

#[gilded::test("tests/ui/eof/*.txt")]
fn eof(test: &mut gilded::Test) {
  #[ilex::spec]
  struct Spec {
    #[rule("/*", "*/")]
    c1: Lexeme<Comment>,

    #[rule("[", "]")]
    b1: Lexeme<Bracket>,

    #[rule("(", ")")]
    b2: Lexeme<Bracket>,

    #[rule(Quoted::new("'"))]
    q1: Lexeme<Quoted>,
  }

  let ctx = Context::new();
  let report =
    ctx.new_report_with(Options { color: false, ..Default::default() });
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Spec::get().spec(), &report) {
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

#[gilded::test("tests/ui/too_small/*.txt")]
fn too_small(test: &mut gilded::Test) {
  #[ilex::spec]
  struct Spec {
    #[rule(Ident::new().prefix("%"))]
    i1: Lexeme<Ident>,
    #[rule(Ident::new().prefix("$").min_len(3))]
    i2: Lexeme<Ident>,

    #[rule(Bracket::rust_style("#", ("r#", "'"), ("'#", "")))]
    r1: Lexeme<Quoted>,
    #[rule(Bracket::rust_style("#", ("q###", "'"), ("'###", "")))]
    r2: Lexeme<Quoted>,

    #[rule(Bracket::cxx_style(Ident::new().min_len(1), ("R'", "("), (")", "'")))]
    c1: Lexeme<Quoted>,
    #[rule(Bracket::cxx_style(Ident::new().min_len(3), ("Q'", "("), (")", "'")))]
    c2: Lexeme<Quoted>,
  }

  let ctx = Context::new();
  let report =
    ctx.new_report_with(Options { color: false, ..Default::default() });
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Spec::get().spec(), &report) {
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

#[gilded::test("tests/ui/unrecognized/*.txt")]
fn unrecognized(test: &mut gilded::Test) {
  #[ilex::spec]
  struct Spec {
    null: Lexeme<Keyword>,

    #[rule("[", "]")]
    cm: Lexeme<Bracket>,
  }

  let ctx = Context::new();
  let report =
    ctx.new_report_with(Options { color: false, ..Default::default() });
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Spec::get().spec(), &report) {
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
