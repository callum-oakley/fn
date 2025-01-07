use anyhow::{anyhow, Context, Result};

macro_rules! with_catch {
    ($scope:expr, $option:expr) => {{
        let res = $option;
        if let Some(exception) = $scope.exception() {
            Err(anyhow!(exception.to_rust_string_lossy(&mut $scope)))
        } else {
            res.context("no exception but empty result")
        }
    }};
}

pub struct Options<'a, I> {
    pub body: &'a str,
    pub env: I,
    pub parse: bool,
    pub stdin: Option<String>,
    pub stringify: bool,
}

pub fn eval<I: Iterator<Item = (String, String)>>(options: Options<'_, I>) -> Result<String> {
    let platform = v8::new_default_platform(0, false).make_shared();
    v8::V8::initialize_platform(platform);
    v8::V8::initialize();
    let mut isolate = v8::Isolate::new(v8::CreateParams::default());
    let mut scope = v8::HandleScope::new(&mut isolate);

    let context = v8::Context::new(&mut scope, v8::ContextOptions::default());
    let mut scope = v8::ContextScope::new(&mut scope, context);
    let mut scope = v8::TryCatch::new(&mut scope);

    let object_template = v8::ObjectTemplate::new(&mut scope);
    for (k, v) in options.env {
        object_template.set(
            string(&mut scope, &format!("${k}"))?.into(),
            string(&mut scope, &v)?.into(),
        );
    }

    if let Some(stdin) = options.stdin {
        let stdin = string(&mut scope, &stdin)?;
        if options.parse {
            let value =
                with_catch!(scope, v8::json::parse(&mut scope, stdin)).context("parsing STDIN")?;
            object_template.set_accessor_with_configuration(
                string(&mut scope, "$")?.into(),
                v8::AccessorConfiguration::new(
                    |_: &mut v8::HandleScope,
                     _: v8::Local<v8::Name>,
                     args: v8::PropertyCallbackArguments,
                     mut rv: v8::ReturnValue<v8::Value>| rv.set(args.data()),
                )
                .data(value),
            );
        } else {
            object_template.set(string(&mut scope, "$")?.into(), stdin.into());
        }
    }

    let context = v8::Context::new(
        &mut scope,
        v8::ContextOptions {
            global_template: Some(object_template),
            ..v8::ContextOptions::default()
        },
    );
    let mut scope = v8::ContextScope::new(&mut scope, context);
    let mut scope = v8::TryCatch::new(&mut scope);

    let script = string(&mut scope, &format!("(() => {})()", options.body))?;
    let script = with_catch!(scope, v8::Script::compile(&mut scope, script, None))
        .context("compiling script")?;

    let mut res = with_catch!(scope, script.run(&mut scope)).context("running script")?;
    if options.stringify {
        res = with_catch!(scope, v8::json::stringify(&mut scope, res))
            .context("stringifying JSON")?
            .into();
    }

    Ok(res.to_rust_string_lossy(&mut scope))
}

fn string<'s>(scope: &mut v8::HandleScope<'s, ()>, s: &str) -> Result<v8::Local<'s, v8::String>> {
    v8::String::new(scope, s).context("constructing string")
}