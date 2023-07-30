use davisjr::prelude::*;
use fancy_duration::FancyDuration;
use rate_limiter::*;
use serde_derive::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config(BTreeMap<String, (FancyDuration<Duration>, usize)>);

impl Config {
    pub fn to_limitmap(&self) -> LimitMap {
        let mut map = LimitMap::default();

        for (route, stuff) in &self.0 {
            map.insert(route.to_string(), (stuff.0.duration(), stuff.1));
        }

        map
    }
}

#[derive(Clone, Default)]
struct AppState {
    limiter: Limiter,
}

impl AppState {
    pub fn new(limiter: Limiter) -> Self {
        Self { limiter }
    }
}

impl TransientState for AppState {
    fn initial() -> Self {
        Default::default()
    }
}

impl HasLimit for AppState {
    fn limiter(&self) -> Limiter {
        self.limiter.clone()
    }
}

pub async fn mock_handler(
    req: Request<Body>,
    _resp: Option<Response<Body>>,
    _params: Params,
    _app: App<impl HasLimit + 'static, NoState>,
    _state: NoState,
) -> HTTPResult<NoState> {
    return Ok((
        req,
        Some(Response::builder().body(Body::empty()).unwrap()),
        _state,
    ));
}

#[tokio::main]
async fn main() -> Result<(), ServerError> {
    let mut io = std::fs::OpenOptions::new();
    io.read(true);
    let io = io
        .open(
            std::env::args()
                .nth(1)
                .expect("Please pass a configuration filename"),
        )
        .expect("Pass a config file with the LimitMap data");

    let config: Config = serde_yaml::from_reader(io).expect("I require a configuration file");

    let limiter = Limiter::new(config.to_limitmap());

    let mut app: App<AppState, NoState> = App::with_state(AppState::new(limiter.clone()));

    for (route, _) in &config.0 {
        app.get(route, compose_handler!(with_limits, mock_handler))?;
    }

    tokio::spawn(async move { observe_limits(limiter).await.unwrap() });

    app.serve("0.0.0.0:8000").await?;
    Ok(())
}
