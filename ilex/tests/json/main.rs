use ilex::fp::Fp64;
use ilex::report::Expected;
use ilex::report::Report;
use ilex::rule::*;
use ilex::token;
use ilex::token::Cursor;
use ilex::Context;
use ilex::Lexeme;

#[ilex::spec]
struct JsonSpec {
  #[rule(",")]
  comma: Lexeme<Keyword>,

  #[rule(":")]
  colon: Lexeme<Keyword>,

  #[rule("true")]
  true_: Lexeme<Keyword>,

  #[rule("false")]
  false_: Lexeme<Keyword>,

  #[rule("null")]
  null: Lexeme<Keyword>,

  #[named]
  #[rule("[", "]")]
  array: Lexeme<Bracket>,

  #[named]
  #[rule("{", "}")]
  object: Lexeme<Bracket>,

  #[named]
  #[rule(Quoted::new('"')
    .invalid_escape(r"\")
    .escapes([
      "\\\"", r"\\", r"\/",
      r"\b", r"\f",  r"\n", r"\t", r"\r",
    ])
    .fixed_length_escape(r"\u", 4))]
  string: Lexeme<Quoted>,

  #[named]
  #[rule(Digital::new(10)
    .minus()
    .point_limit(0..2)
    .exponents(["e", "E"], Digits::new(10).plus().minus()))]
  number: Lexeme<Digital>,
}

#[gilded::test("tests/json/*.json")]
fn check_tokens(test: &mut gilded::Test) {
  let ctx = Context::new();
  let report = ctx.new_report();
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  let stream = match file.lex(JsonSpec::get().spec(), &report) {
    Ok(stream) => stream,
    Err(fatal) => {
      test.output("tokens.yaml", "".into());
      test.output("ast.txt", "".into());
      test.output("stderr", format!("{fatal:?}"));
      return;
    }
  };

  test.output("tokens.yaml", stream.summary());

  let json = parse(&report, JsonSpec::get(), &mut stream.cursor());
  if let Err(fatal) = report.fatal_or(()) {
    test.output("ast.txt", "".into());
    test.output("stderr", format!("{fatal:?}"));
    return;
  }

  test.output("ast.txt", format!("{json:#?}"));
  test.output("stderr", "".into());
}

#[derive(Clone, Debug, PartialEq)]
enum Json {
  Null,
  Num(f64),
  Bool(bool),
  Str(String),
  Arr(Vec<Json>),
  Obj(Vec<(String, Json)>),
}

fn parse(report: &Report, json: &JsonSpec, cursor: &mut Cursor) -> Json {
  let quote2str = |str: token::Quoted| -> String {
    str.to_utf8(|key, data, buf| {
      let char = match key.text() {
        "\\\"" => '\"',
        r"\\" => '\\',
        r"\/" => '/',
        r"\b" => '\x08',
        r"\f" => '\x0c',
        r"\n" => '\n',
        r"\t" => '\t',
        r"\r" => '\r',
        // This is sloppy about surrogates but this is just an example.
        r"\u" => {
          let data = data.unwrap();
          let code =
            u16::from_str_radix(data.text(), 16).unwrap_or_else(|_| {
              report.builtins(json.spec()).expected(
                [Expected::Name("hex-encoded u16".into())],
                data.text(),
                data,
              );
              0
            });
          for c in char::decode_utf16([code]) {
            buf.push(c.unwrap_or('😢'))
          }
          return;
        }
        esc => panic!("{}", esc),
      };
      buf.push(char);
    })
  };

  let value = token::switch()
    .case(json.null, |_, _| Json::Null)
    .case(json.false_, |_, _| Json::Bool(false))
    .case(json.true_, |_, _| Json::Bool(true))
    .case(json.string, |str: token::Quoted, _| Json::Str(quote2str(str)))
    .case(json.number, |num: token::Digital, _| {
      Json::Num(num.to_float::<Fp64>(.., report).unwrap().to_hard())
    })
    .case(json.array, |array: token::Bracket, _| {
      let mut trailing = None;
      let vec = array
        .contents()
        .delimited(json.comma, |c| Some(parse(report, json, c)))
        .map(|(e, c)| {
          trailing = c;
          e
        })
        .collect();

      if let Some(comma) = trailing {
        report
          .error("trailing commas are not allowed in JSON")
          .saying(comma, "remove this comma");
      }

      Json::Arr(vec)
    })
    .case(json.object, |object: token::Bracket, _| {
      let mut trailing = None;
      let vec = object
        .contents()
        .delimited(json.comma, |c| {
          let key = c
            .take(json.string, report)
            .map(|q| quote2str(q))
            .unwrap_or("😢".into());
          c.take(json.colon, report);
          let value = parse(report, json, c);
          Some((key, value))
        })
        .map(|(e, c)| {
          trailing = c;
          e
        })
        .collect();

      if let Some(comma) = trailing {
        report
          .error("trailing commas are not allowed in JSON")
          .saying(comma, "remove this comma");
      }

      Json::Obj(vec)
    })
    .take(cursor, report);
  value.unwrap_or(Json::Null)
}
