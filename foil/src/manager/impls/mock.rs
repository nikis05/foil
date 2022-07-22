use std::str::FromStr;

use crate::manager::impls::log::{Error, LogManager};
use crate::manager::Record;
use crate::Manager;
use futures::stream::BoxStream;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Executor, Sqlite, SqliteConnection};

pub struct MockManager {
    history: Vec<String>,
    conn: SqliteConnection,
}

impl MockManager {
    pub async fn new() -> sqlx::Result<Self> {
        let conn = SqliteConnectOptions::from_str("sqlite::memory:")?
            .connect()
            .await?;
        Ok(Self {
            history: vec![],
            conn,
        })
    }

    pub async fn exec_sql(&mut self, sql: &str) -> sqlx::Result<sqlx::sqlite::SqliteQueryResult> {
        let result = self.conn.execute(sql).await?;
        self.history.push(sql.into());
        Ok(result)
    }

    pub fn last_statement(&self) -> Option<&str> {
        self.history.last().map(String::as_str)
    }
}

macro_rules! record_and_delegate {
    ($self:expr, $query:expr, $method:ident) => {{
        let mut sql = String::new();
        let manager = LogManager::new(&mut $self.conn, &mut sql);
        let stream = manager.$method($query);
        $self.history.push(sql);
        stream
    }};
}

impl<'m> Manager<'m, Sqlite> for &'m mut MockManager {
    type Error = Error<sqlx::Error>;

    fn select<'q, 'o>(
        self,
        query: crate::manager::SelectQuery<'q, Sqlite>,
    ) -> futures::stream::BoxStream<'o, Result<crate::manager::Record<Sqlite>, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        record_and_delegate!(self, query, select)
    }

    fn count<'q, 'o>(
        self,
        query: crate::manager::CountQuery<'q, Sqlite>,
    ) -> futures::future::BoxFuture<'o, Result<i64, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
        for<'a> i64: sqlx::Type<Sqlite> + sqlx::Decode<'a, Sqlite>,
        for<'a> &'a str: sqlx::ColumnIndex<<Sqlite as sqlx::Database>::Row>,
    {
        record_and_delegate!(self, query, count)
    }

    fn insert<'q, 'o>(
        self,
        query: crate::manager::InsertQuery<'q, Sqlite>,
    ) -> futures::future::BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        record_and_delegate!(self, query, insert)
    }

    fn insert_returning<'q, 'o>(
        self,
        query: crate::manager::InsertReturningQuery<'q, Sqlite>,
    ) -> futures::stream::BoxStream<'o, Result<crate::manager::Record<Sqlite>, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        record_and_delegate!(self, query, insert_returning)
    }

    fn update<'q, 'o>(
        self,
        query: crate::manager::UpdateQuery<'q, Sqlite>,
    ) -> futures::future::BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        record_and_delegate!(self, query, update)
    }

    fn delete<'q, 'o>(
        self,
        query: crate::manager::DeleteQuery<'q, Sqlite>,
    ) -> futures::future::BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        record_and_delegate!(self, query, delete)
    }

    fn query<'q, 'o, Q: sqlx::Execute<'q, Sqlite> + 'q>(
        self,
        query: Q,
    ) -> BoxStream<'o, Result<Record<Sqlite>, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        record_and_delegate!(self, query, query)
    }
}
