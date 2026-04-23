use ignis_sdk::http::{Context, Router, middleware, text_response};
use ignis_sdk::mysql::{self, MysqlValue, Statement as MysqlStatement};
use ignis_sdk::postgres::{self, PostgresValue, Statement as PostgresStatement};
use wstd::http::{Body, Request, Response, Result, StatusCode};

#[wstd::http_server]
async fn main(req: Request<Body>) -> Result<Response<Body>> {
    let router = build_router();
    Ok(router.handle(req).await)
}

fn build_router() -> Router {
    let mut router = Router::new();
    router.use_middleware(middleware::request_id());
    router.use_middleware(middleware::logger());

    router
        .get("/", |_context: Context| async move {
            match run_postgres_healthcheck() {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("postgres", error),
            }
        })
        .expect("register GET /");
    router
        .get("/postgres", |_context: Context| async move {
            match run_postgres_healthcheck() {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("postgres", error),
            }
        })
        .expect("register GET /postgres");
    router
        .post("/increment", |_context: Context| async move {
            match increment_postgres_counter("manual increment") {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("postgres", error),
            }
        })
        .expect("register POST /increment");
    router
        .post("/transaction-smoke", |_context: Context| async move {
            match increment_postgres_counter("transaction smoke") {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("postgres", error),
            }
        })
        .expect("register POST /transaction-smoke");
    router
        .post("/reset", |_context: Context| async move {
            match reset_postgres_counter() {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("postgres", error),
            }
        })
        .expect("register POST /reset");

    router
        .get("/mysql", |_context: Context| async move {
            match run_mysql_healthcheck() {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("mysql", error),
            }
        })
        .expect("register GET /mysql");
    router
        .post("/mysql/increment", |_context: Context| async move {
            match increment_mysql_counter("manual increment") {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("mysql", error),
            }
        })
        .expect("register POST /mysql/increment");
    router
        .post("/mysql/transaction-smoke", |_context: Context| async move {
            match increment_mysql_counter("transaction smoke") {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("mysql", error),
            }
        })
        .expect("register POST /mysql/transaction-smoke");
    router
        .post("/mysql/bulk-smoke", |_context: Context| async move {
            match bulk_mysql_smoke(64) {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("mysql", error),
            }
        })
        .expect("register POST /mysql/bulk-smoke");
    router
        .post("/mysql/reset", |_context: Context| async move {
            match reset_mysql_counter() {
                Ok(summary) => text_response(StatusCode::OK, summary),
                Err(error) => error_response("mysql", error),
            }
        })
        .expect("register POST /mysql/reset");

    router
}

fn run_postgres_healthcheck() -> std::result::Result<String, String> {
    ensure_postgres_schema()?;
    ensure_postgres_seed()?;
    assert_postgres_type_roundtrip()?;
    render_postgres_summary("healthcheck")
}

fn ensure_postgres_schema() -> std::result::Result<(), String> {
    postgres::execute(
        "create table if not exists counters (
            name text primary key,
            value bigint not null default 0
        )",
        &[],
    )?;
    postgres::execute(
        "create table if not exists counter_events (
            id bigserial primary key,
            message text not null,
            created_at timestamptz not null default now()
        )",
        &[],
    )?;
    Ok(())
}

fn ensure_postgres_seed() -> std::result::Result<(), String> {
    postgres::execute(
        "insert into counters (name, value) values ($1, 0) on conflict (name) do nothing",
        &[PostgresValue::Text("hits".to_owned())],
    )?;
    Ok(())
}

fn increment_postgres_counter(message: &str) -> std::result::Result<String, String> {
    ensure_postgres_schema()?;
    ensure_postgres_seed()?;
    let changed = postgres::transaction(&[
        PostgresStatement {
            sql: "update counters set value = value + 1 where name = $1".to_owned(),
            params: vec![PostgresValue::Text("hits".to_owned())],
        },
        PostgresStatement {
            sql: "insert into counter_events (message) values ($1)".to_owned(),
            params: vec![PostgresValue::Text(message.to_owned())],
        },
    ])?;
    assert_postgres_type_roundtrip()?;
    render_postgres_summary(&format!("transaction_changed={changed}"))
}

fn reset_postgres_counter() -> std::result::Result<String, String> {
    ensure_postgres_schema()?;
    postgres::transaction(&[
        PostgresStatement {
            sql: "insert into counters (name, value) values ($1, 0) on conflict (name) do nothing"
                .to_owned(),
            params: vec![PostgresValue::Text("hits".to_owned())],
        },
        PostgresStatement {
            sql: "update counters set value = 0 where name = $1".to_owned(),
            params: vec![PostgresValue::Text("hits".to_owned())],
        },
        PostgresStatement {
            sql: "delete from counter_events".to_owned(),
            params: vec![],
        },
    ])?;
    render_postgres_summary("reset")
}

fn render_postgres_summary(action: &str) -> std::result::Result<String, String> {
    let counter = read_postgres_i64(
        "select value from counters where name = $1",
        &[PostgresValue::Text("hits".to_owned())],
    )?;
    let events = read_postgres_i64("select count(*) from counter_events", &[])?;
    let last_event = read_postgres_optional_text(
        "select message from counter_events order by id desc limit 1",
        &[],
    )?
    .unwrap_or_else(|| "none".to_owned());

    Ok(format!(
        "postgres=ok\naction={action}\ncounter={counter}\nevents={events}\nlast_event={last_event}\ntypes=ok\n"
    ))
}

fn assert_postgres_type_roundtrip() -> std::result::Result<(), String> {
    let result = postgres::query(
        "select $1::text as text_value,
                $2::bigint as int_value,
                $3::double precision as float_value,
                $4::boolean as bool_value",
        &[
            PostgresValue::Text("hello-postgres".to_owned()),
            PostgresValue::Integer(42),
            PostgresValue::Float(3.5),
            PostgresValue::Boolean(true),
        ],
    )?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "postgres type smoke query returned no rows".to_owned())?;
    expect_postgres_text(row.values.first(), "hello-postgres")?;
    expect_postgres_integer(row.values.get(1), 42)?;
    expect_postgres_float(row.values.get(2), 3.5)?;
    expect_postgres_boolean(row.values.get(3), true)?;
    Ok(())
}

fn read_postgres_i64(sql: &str, params: &[PostgresValue]) -> std::result::Result<i64, String> {
    let result = postgres::query(sql, params)?;
    let value = result
        .rows
        .first()
        .and_then(|row| row.values.first())
        .ok_or_else(|| format!("query returned no value: {sql}"))?;
    match value {
        PostgresValue::Integer(value) => Ok(*value),
        other => Err(format!("expected postgres integer result, got {other:?}")),
    }
}

fn read_postgres_optional_text(
    sql: &str,
    params: &[PostgresValue],
) -> std::result::Result<Option<String>, String> {
    let result = postgres::query(sql, params)?;
    let Some(value) = result.rows.first().and_then(|row| row.values.first()) else {
        return Ok(None);
    };
    match value {
        PostgresValue::Text(value) => Ok(Some(value.clone())),
        PostgresValue::Null => Ok(None),
        other => Err(format!("expected postgres text result, got {other:?}")),
    }
}

fn run_mysql_healthcheck() -> std::result::Result<String, String> {
    ensure_mysql_schema()?;
    ensure_mysql_seed()?;
    assert_mysql_type_roundtrip()?;
    render_mysql_summary("healthcheck")
}

fn ensure_mysql_schema() -> std::result::Result<(), String> {
    mysql::execute(
        "create table if not exists ignis_mysql_counters (
            name varchar(64) primary key,
            value bigint not null default 0
        ) engine=InnoDB",
        &[],
    )?;
    mysql::execute(
        "create table if not exists ignis_mysql_events (
            id bigint unsigned not null auto_increment primary key,
            message varchar(255) not null,
            created_at timestamp not null default current_timestamp
        ) engine=InnoDB",
        &[],
    )?;
    Ok(())
}

fn ensure_mysql_seed() -> std::result::Result<(), String> {
    mysql::execute(
        "insert into ignis_mysql_counters (name, value)
         values (?, 0)
         on duplicate key update value = value",
        &[MysqlValue::Text("hits".to_owned())],
    )?;
    Ok(())
}

fn increment_mysql_counter(message: &str) -> std::result::Result<String, String> {
    ensure_mysql_schema()?;
    ensure_mysql_seed()?;
    let changed = mysql::transaction(&[
        MysqlStatement {
            sql: "update ignis_mysql_counters set value = value + 1 where name = ?".to_owned(),
            params: vec![MysqlValue::Text("hits".to_owned())],
        },
        MysqlStatement {
            sql: "insert into ignis_mysql_events (message) values (?)".to_owned(),
            params: vec![MysqlValue::Text(message.to_owned())],
        },
    ])?;
    assert_mysql_type_roundtrip()?;
    render_mysql_summary(&format!("transaction_changed={changed}"))
}

fn bulk_mysql_smoke(iterations: usize) -> std::result::Result<String, String> {
    ensure_mysql_schema()?;
    ensure_mysql_seed()?;
    let mut statements = Vec::with_capacity(iterations.saturating_mul(2));
    for index in 0..iterations {
        statements.push(MysqlStatement {
            sql: "update ignis_mysql_counters set value = value + 1 where name = ?".to_owned(),
            params: vec![MysqlValue::Text("hits".to_owned())],
        });
        statements.push(MysqlStatement {
            sql: "insert into ignis_mysql_events (message) values (?)".to_owned(),
            params: vec![MysqlValue::Text(format!("bulk smoke {index}"))],
        });
    }
    let changed = mysql::transaction(&statements)?;
    render_mysql_summary(&format!("bulk_changed={changed}"))
}

fn reset_mysql_counter() -> std::result::Result<String, String> {
    ensure_mysql_schema()?;
    mysql::transaction(&[
        MysqlStatement {
            sql: "insert into ignis_mysql_counters (name, value)
                 values (?, 0)
                 on duplicate key update value = value"
                .to_owned(),
            params: vec![MysqlValue::Text("hits".to_owned())],
        },
        MysqlStatement {
            sql: "update ignis_mysql_counters set value = 0 where name = ?".to_owned(),
            params: vec![MysqlValue::Text("hits".to_owned())],
        },
        MysqlStatement {
            sql: "delete from ignis_mysql_events".to_owned(),
            params: vec![],
        },
    ])?;
    render_mysql_summary("reset")
}

fn render_mysql_summary(action: &str) -> std::result::Result<String, String> {
    let counter = read_mysql_i64(
        "select value from ignis_mysql_counters where name = ?",
        &[MysqlValue::Text("hits".to_owned())],
    )?;
    let events = read_mysql_i64("select count(*) from ignis_mysql_events", &[])?;
    let last_event = read_mysql_optional_text(
        "select message from ignis_mysql_events order by id desc limit 1",
        &[],
    )?
    .unwrap_or_else(|| "none".to_owned());

    Ok(format!(
        "mysql=ok\naction={action}\ncounter={counter}\nevents={events}\nlast_event={last_event}\ntypes=ok\npool=host-side\n"
    ))
}

fn assert_mysql_type_roundtrip() -> std::result::Result<(), String> {
    let result = mysql::query(
        "select cast(? as char) as text_value,
                cast(? as signed) as int_value,
                cast(? as double) as float_value,
                if(? = true, true, false) as bool_value",
        &[
            MysqlValue::Text("hello-mysql".to_owned()),
            MysqlValue::Integer(42),
            MysqlValue::Float(3.5),
            MysqlValue::Boolean(true),
        ],
    )?;
    let row = result
        .rows
        .first()
        .ok_or_else(|| "mysql type smoke query returned no rows".to_owned())?;
    expect_mysql_text(row.values.first(), "hello-mysql")?;
    expect_mysql_integer(row.values.get(1), 42)?;
    expect_mysql_float(row.values.get(2), 3.5)?;
    expect_mysql_boolean(row.values.get(3), true)?;
    Ok(())
}

fn read_mysql_i64(sql: &str, params: &[MysqlValue]) -> std::result::Result<i64, String> {
    let result = mysql::query(sql, params)?;
    let value = result
        .rows
        .first()
        .and_then(|row| row.values.first())
        .ok_or_else(|| format!("query returned no value: {sql}"))?;
    match value {
        MysqlValue::Integer(value) => Ok(*value),
        MysqlValue::Boolean(value) => Ok(i64::from(*value)),
        other => Err(format!("expected mysql integer result, got {other:?}")),
    }
}

fn read_mysql_optional_text(
    sql: &str,
    params: &[MysqlValue],
) -> std::result::Result<Option<String>, String> {
    let result = mysql::query(sql, params)?;
    let Some(value) = result.rows.first().and_then(|row| row.values.first()) else {
        return Ok(None);
    };
    match value {
        MysqlValue::Text(value) => Ok(Some(value.clone())),
        MysqlValue::Null => Ok(None),
        other => Err(format!("expected mysql text result, got {other:?}")),
    }
}

fn expect_postgres_text(
    value: Option<&PostgresValue>,
    expected: &str,
) -> std::result::Result<(), String> {
    match value {
        Some(PostgresValue::Text(value)) if value == expected => Ok(()),
        other => Err(format!("expected postgres text {expected:?}, got {other:?}")),
    }
}

fn expect_postgres_integer(
    value: Option<&PostgresValue>,
    expected: i64,
) -> std::result::Result<(), String> {
    match value {
        Some(PostgresValue::Integer(value)) if *value == expected => Ok(()),
        other => Err(format!("expected postgres integer {expected}, got {other:?}")),
    }
}

fn expect_postgres_float(
    value: Option<&PostgresValue>,
    expected: f64,
) -> std::result::Result<(), String> {
    match value {
        Some(PostgresValue::Float(value)) if (*value - expected).abs() < f64::EPSILON => Ok(()),
        other => Err(format!("expected postgres float {expected}, got {other:?}")),
    }
}

fn expect_postgres_boolean(
    value: Option<&PostgresValue>,
    expected: bool,
) -> std::result::Result<(), String> {
    match value {
        Some(PostgresValue::Boolean(value)) if *value == expected => Ok(()),
        other => Err(format!("expected postgres boolean {expected}, got {other:?}")),
    }
}

fn expect_mysql_text(
    value: Option<&MysqlValue>,
    expected: &str,
) -> std::result::Result<(), String> {
    match value {
        Some(MysqlValue::Text(value)) if value == expected => Ok(()),
        other => Err(format!("expected mysql text {expected:?}, got {other:?}")),
    }
}

fn expect_mysql_integer(
    value: Option<&MysqlValue>,
    expected: i64,
) -> std::result::Result<(), String> {
    match value {
        Some(MysqlValue::Integer(value)) if *value == expected => Ok(()),
        other => Err(format!("expected mysql integer {expected}, got {other:?}")),
    }
}

fn expect_mysql_float(value: Option<&MysqlValue>, expected: f64) -> std::result::Result<(), String> {
    match value {
        Some(MysqlValue::Float(value)) if (*value - expected).abs() < f64::EPSILON => Ok(()),
        other => Err(format!("expected mysql float {expected}, got {other:?}")),
    }
}

fn expect_mysql_boolean(
    value: Option<&MysqlValue>,
    expected: bool,
) -> std::result::Result<(), String> {
    match value {
        Some(MysqlValue::Boolean(value)) if *value == expected => Ok(()),
        Some(MysqlValue::Integer(value)) if (*value != 0) == expected => Ok(()),
        other => Err(format!("expected mysql boolean {expected}, got {other:?}")),
    }
}

fn error_response(database: &str, error: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(format!("{database} error: {error}\n").into())
        .expect("error response")
}
