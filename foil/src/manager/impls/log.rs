use crate::{
    manager::{display::WithBindParameters, Record},
    Manager,
};
use futures::{stream::BoxStream, TryFutureExt, TryStreamExt};
use sqlx::Database;
use thiserror::Error;

pub struct LogManager<M, W: std::fmt::Write + Send> {
    inner: M,
    writer: W,
}

impl<M, W: std::fmt::Write + Send> LogManager<M, W> {
    pub fn new(inner: M, writer: W) -> Self {
        Self { inner, writer }
    }
}

impl<'m, DB: Database + WithBindParameters, M: Manager<'m, DB>, W: std::fmt::Write + Send>
    Manager<'m, DB> for LogManager<M, W>
{
    type Error = Error<M::Error>;

    fn select<'q, 'o>(
        mut self,
        query: crate::manager::SelectQuery<'q, DB>,
    ) -> futures::stream::BoxStream<'o, Result<crate::manager::Record<DB>, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        if let Err(err) = write!(self.writer, "{}", query) {
            return Box::pin(futures::stream::once(async move { Err(err.into()) }));
        }

        Box::pin(self.inner.select(query).map_err(Error::Inner))
    }

    fn count<'q, 'o>(
        mut self,
        query: crate::manager::CountQuery<'q, DB>,
    ) -> futures::future::BoxFuture<'o, Result<u32, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
        for<'a> u32: sqlx::Type<DB> + sqlx::Decode<'a, DB>,
        for<'a> &'a str: sqlx::ColumnIndex<<DB as Database>::Row>,
    {
        if let Err(err) = write!(self.writer, "{}", query) {
            return Box::pin(futures::future::ready(Err(err.into())));
        }

        Box::pin(self.inner.count(query).map_err(Error::Inner))
    }

    fn insert<'q, 'o>(
        mut self,
        query: crate::manager::InsertQuery<'q, DB>,
    ) -> futures::future::BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        if let Err(err) = write!(self.writer, "{}", query) {
            return Box::pin(futures::future::ready(Err(err.into())));
        }

        Box::pin(self.inner.insert(query).map_err(Error::Inner))
    }

    fn insert_returning<'q, 'o>(
        mut self,
        query: crate::manager::InsertReturningQuery<'q, DB>,
    ) -> futures::stream::BoxStream<'o, Result<crate::manager::Record<DB>, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        if let Err(err) = write!(self.writer, "{}", query) {
            return Box::pin(futures::stream::once(async move { Err(err.into()) }));
        }

        Box::pin(self.inner.insert_returning(query).map_err(Error::Inner))
    }

    fn update<'q, 'o>(
        mut self,
        query: crate::manager::UpdateQuery<'q, DB>,
    ) -> futures::future::BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        if let Err(err) = write!(self.writer, "{}", query) {
            return Box::pin(futures::future::ready(Err(err.into())));
        }

        Box::pin(self.inner.update(query).map_err(Error::Inner))
    }

    fn delete<'q, 'o>(
        mut self,
        query: crate::manager::DeleteQuery<'q, DB>,
    ) -> futures::future::BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        if let Err(err) = write!(self.writer, "{}", query) {
            return Box::pin(futures::future::ready(Err(err.into())));
        }

        Box::pin(self.inner.delete(query).map_err(Error::Inner))
    }

    fn query<'q, 'o, Q: sqlx::Execute<'q, DB> + 'q>(
        mut self,
        query: Q,
    ) -> BoxStream<'o, Result<Record<DB>, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
    {
        if let Err(err) = write!(self.writer, "{}", query.sql()) {
            return Box::pin(futures::stream::once(async move { Err(err.into()) }));
        }

        Box::pin(self.inner.query(query).map_err(Error::Inner))
    }
}

#[derive(Debug, Error)]
pub enum Error<Inner> {
    #[error("Error logging SQL: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error(transparent)]
    Inner(Inner),
}
