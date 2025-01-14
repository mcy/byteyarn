use ilex::fp::Fp64;
use ilex::report::Report;
use ilex::rule::*;
use ilex::token;
use ilex::Context;
use ilex::Lexeme;

#[ilex::spec]
struct Numbers {
  #[rule(",")]
  comma: Lexeme<Keyword>,

  #[named("binary number")]
  #[rule(Digital::new(2)
    .separator('_')
    .plus().minus()
    .point_limit(0..2)
    .exponent("2", Digits::new(2).plus().minus())
    .prefixes(["0b", "0B", "%"]))]
  bin: Lexeme<Digital>,

  #[named = "hexadecimal number"]
  #[rule(Digital::new(16)
    .separator('_')
    .plus().minus()
    .point_limit(0..2)
    .exponents(["p", "P"], Digits::new(10).plus().minus())
    .prefixes(["0x", "0X", "$"]))]
  hex: Lexeme<Digital>,

  #[named = "quaternary number"]
  #[rule(Digital::new(4)
    .separator('_')
    .plus().minus()
    .point_limit(0..2)
    .exponents(["p", "P"], Digits::new(10).plus().minus())
    .prefixes(["0q", "0Q"]))]
  qua: Lexeme<Digital>,

  #[named = "octal number"]
  #[rule(Digital::new(8)
    .separator('_')
    .plus().minus()
    .point_limit(0..2)
    .exponents(["p", "P"], Digits::new(10).plus().minus())
    .prefixes(["0o", "0O", "0"]))]
  oct: Lexeme<Digital>,

  #[named = "decimal number"]
  #[rule(Digital::new(10)
    .separator('_')
    .plus().minus()
    .point_limit(0..2)
    .exponents(["e", "E"], Digits::new(10).plus().minus())
    .exponent("^", Digits::new(16).plus().minus()))]
  dec: Lexeme<Digital>,
}

#[gilded::test("tests/numbers/*.txt")]
fn numbers(test: &mut gilded::Test) {
  let ctx = Context::new();
  let report = ctx.new_report();
  let file = ctx
    .new_file_from_bytes(test.path(), test.text(), &report)
    .unwrap();

  match file.lex(Numbers::get().spec(), &report) {
    Ok(stream) => {
      test.output("tokens.yaml", stream.summary());
      match parse(Numbers::get(), stream.cursor(), &report) {
        Ok(v) => {
          test.output("fp64.txt", format!("{v:#?}"));
          test.output("stderr", "".into())
        }
        Err(fatal) => {
          test.output("fp64.txt", "".into());
          test.output("stderr", format!("{fatal:?}"));
        }
      }
    }
    Err(fatal) => {
      test.output("tokens.yaml", "".into());
      test.output("stderr", format!("{fatal:?}"));
      test.output("fp64.txt", "".into());
    }
  }
}

fn parse(
  lex: &Numbers,
  mut cursor: ilex::token::Cursor,
  report: &Report,
) -> Result<Vec<Fp64>, ilex::report::Fatal> {
  let numbers = cursor
    .delimited(lex.comma, |cursor| loop {
      let value = token::switch()
        .case(Lexeme::eof(), |_, _| Err(false))
        .cases([lex.dec, lex.bin, lex.oct, lex.hex, lex.qua], |num, _| {
          Ok(num.to_float::<Fp64>(.., report).unwrap())
        })
        .take(cursor, report);
      match value {
        None => {
          cursor.back_up(1);
          return Some(Fp64::nan());
        }
        Some(Err(false)) => return None,
        Some(Err(true)) => continue,
        Some(Ok(v)) => return Some(v),
      }
    })
    .map(|(v, _)| v)
    .collect::<Vec<_>>();
  cursor.expect_finished(report);
  report.fatal_or(numbers)
}
