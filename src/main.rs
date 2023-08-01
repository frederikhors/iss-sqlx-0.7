/*
[dependencies]
async-trait = { version = "0.1.68", default-features = false }
futures = "0.3.28"
sqlx = { version = "0.7.1", default-features = false, features = [
    "macros",
    "postgres",
    "runtime-tokio-rustls",
    "time",
] }
tokio = { version = "1.28.2", default-features = false, features = ["macros", "rt-multi-thread"] }
*/

use futures::future::{BoxFuture, FutureExt};
use std::collections::BTreeMap;

#[async_trait::async_trait]
pub trait Trait: Send + Sync + TeamTrait {
    fn clone_boxed(&self) -> Box<dyn Trait>;
    async fn start_transaction(
        &self,
    ) -> Result<Box<dyn TransactionTrait>, Box<dyn std::error::Error + Send + Sync + 'static>>;
}

#[async_trait::async_trait]
pub trait TransactionTrait: Send + Sync + TeamTrait {
    async fn commit(
        self: Box<Self>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
}

#[async_trait::async_trait]
pub trait TeamTrait {
    async fn team_list(
        &mut self,
        user: &str,
    ) -> Result<Vec<Team>, Box<dyn std::error::Error + Send + Sync + 'static>>;
}

#[derive(Clone)]
pub struct Repo<E = sqlx::PgPool> {
    db: E,
}

impl Repo {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { db: pool }
    }
}

#[async_trait::async_trait]
impl Trait for Repo {
    fn clone_boxed(&self) -> Box<dyn Trait> {
        Box::new(Clone::clone(self))
    }

    async fn start_transaction(
        &self,
    ) -> Result<Box<dyn TransactionTrait>, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let tx = self.db.begin().await?;

        Ok(Box::new(Repo { db: tx }))
    }
}

#[async_trait::async_trait]
impl TransactionTrait for Repo<sqlx::Transaction<'static, sqlx::Postgres>> {
    async fn commit(
        self: Box<Self>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.db.commit().await?;

        Ok(())
    }
}

pub trait Executor: Send + Sync {
    type Executor<'this>: Send + Sync + sqlx::PgExecutor<'this>;

    // From https://users.rust-lang.org/t/why-does-this-impl-executor-does-not-live-long-enough/94241
    fn _disable_lint(e: Self::Executor<'_>) -> Self::Executor<'_>;

    fn as_executor(&mut self) -> Self::Executor<'_>;
}

impl Executor for sqlx::PgPool {
    type Executor<'this> = &'this Self;

    fn _disable_lint(e: Self::Executor<'_>) -> Self::Executor<'_> {
        e
    }

    fn as_executor(&mut self) -> Self::Executor<'_> {
        self
    }
}

impl Executor for sqlx::Transaction<'static, sqlx::Postgres> {
    type Executor<'this> = &'this mut Self;

    fn _disable_lint(e: Self::Executor<'_>) -> Self::Executor<'_> {
        e
    }

    fn as_executor(&mut self) -> Self::Executor<'_> {
        self
    }
}

#[async_trait::async_trait]
impl<E: 'static + Executor> TeamTrait for Repo<E> {
    async fn team_list(
        &mut self,
        user: &str,
    ) -> Result<Vec<Team>, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let store = query_teams(user, &mut self.db, None, true).await;

        Ok(store.teams.values().map(|team| (*team).clone()).collect())
    }
}

#[derive(Default)]
pub struct Store {
    pub teams: BTreeMap<String, Team>,
    pub players: BTreeMap<String, Player>,
}

#[derive(Default, Clone)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub team_id: String,
}

#[derive(Default, Clone, sqlx::FromRow)]
pub struct Team {
    pub id: String,
    pub name: String,
}

fn query_teams<'a>(
    user: &'a str,
    db: &'a mut impl Executor,
    store: Option<Store>,
    with_player: bool,
) -> BoxFuture<'a, Store> {
    async move {
        dbg!("I'm:", user);

        let mut store = store.unwrap_or_default();

        // let teams = vec![
        //     Team {
        //         id: "1".to_string(),
        //         name: "One".to_string(),
        //     },
        //     Team {
        //         id: "2".to_string(),
        //         name: "Two".to_string(),
        //     },
        // ];

        let mut query = sqlx::QueryBuilder::new(r#"SELECT "teams".*" FROM "teams"#);

        let teams = query
            .build_query_as::<Team>()
            .fetch_all(db.as_executor())
            .await
            .unwrap();

        for team in teams {
            store.teams.insert(team.id.to_string(), team);
        }

        if with_player {
            store = query_players(user, db, Some(store), false).await;
        }

        store
    }
    .boxed()
}

async fn query_players(
    user: &str,
    db: &mut impl Executor,
    store: Option<Store>,
    with_team: bool,
) -> Store {
    dbg!("I'm:", user);

    let mut store = store.unwrap_or_default();

    let players = vec![
        Player {
            id: "1".to_string(),
            name: "Bob".to_string(),
            team_id: "1".to_string(),
        },
        Player {
            id: "2".to_string(),
            name: "John".to_string(),
            team_id: "2".to_string(),
        },
    ];

    for player in players {
        store.players.insert(player.id.to_string(), player);
    }

    if with_team {
        store = query_teams(user, db, Some(store), false).await;
    }

    store
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let pg_pool = sqlx::PgPool::connect("postgres://postgres:postgres@localhost").await?;

    let mut pg_repo = Repo::new(pg_pool);

    pg_repo.team_list("me").await?;

    Ok(())
}
