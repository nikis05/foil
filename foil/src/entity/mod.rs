use crate::manager::{
    Condition, CountQuery, DeleteQuery, FindOperator, FromRecord, InsertQuery,
    InsertReturningQuery, IntoCondition, Manager, Order, Record, RecordError, SelectQuery,
    ToValues, UpdateQuery, Value,
};
use futures::{
    future::BoxFuture, stream::BoxStream, FutureExt, StreamExt, TryFutureExt, TryStreamExt,
};
use sqlx::Database;
use std::error::Error;
use std::marker::PhantomData;
use thiserror::Error;
use vec1::{vec1, Vec1};

#[cfg(all(
    test,
    feature = "test-manager",
    feature = "runtime-tokio-rustls",
    feature = "uuid",
    feature = "tokio"
))]
mod test;

pub trait Entity<DB: Database>: FromRecord<DB> + 'static {
    type Id: Send + Value<DB>;
    type Cond: Send + IntoCondition<DB>;
    type Col: Send + Col;

    fn table_name() -> &'static str;

    fn id_col_name() -> &'static str;

    fn col_names() -> Vec1<&'static str>;

    fn id(&self) -> Self::Id;

    fn get<'m, M: Manager<'m, DB>>(
        manager: M,
        id: Self::Id,
    ) -> BoxFuture<'m, Result<Self, SelectOneError<M::Error>>> {
        let mut cond = Condition::default();
        cond.add_col(Self::id_col_name(), FindOperator::Eq(Box::new(id)));

        Box::pin(
            Selection::<_, Self, DB>::new(manager.select(SelectQuery {
                table_name: Self::table_name(),
                col_names: Self::col_names(),
                conds: vec1![cond],
                order_by: None,
                offset: None,
                limit: None,
            }))
            .one(),
        )
    }

    fn find<'m, M: Manager<'m, DB>>(
        manager: M,
        conds: Vec1<Self::Cond>,
    ) -> Selection<'m, M::Error, Self, DB> {
        Self::find_with(
            manager,
            conds,
            FindOptions {
                order_by: None,
                offset: None,
                limit: None,
            },
        )
    }

    fn find_with<'m, M: Manager<'m, DB>>(
        manager: M,
        conds: Vec1<Self::Cond>,
        options: FindOptions<Self::Col>,
    ) -> Selection<'m, M::Error, Self, DB> {
        Selection::new(
            manager.select(SelectQuery {
                table_name: Self::table_name(),
                col_names: Self::col_names(),
                conds: conds.mapped(|cond| cond.into_condition()),
                order_by: options
                    .order_by
                    .map(|order_by| (order_by.cols.mapped(|col| col.as_str()), order_by.order)),
                offset: options.offset,
                limit: options.limit,
            }),
        )
    }

    fn count<'m, M: Manager<'m, DB>>(
        manager: M,
        conds: Vec1<Self::Cond>,
    ) -> BoxFuture<'m, Result<u32, M::Error>>
    where
        for<'a> u32: sqlx::Type<DB> + sqlx::Decode<'a, DB>,
        for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>,
    {
        manager.count(CountQuery {
            table_name: Self::table_name(),
            conds: conds.mapped(|cond| cond.into_condition()),
        })
    }

    fn exists<'m, M: Manager<'m, DB>>(
        manager: M,
        conds: Vec1<Self::Cond>,
    ) -> BoxFuture<'m, Result<bool, M::Error>>
    where
        for<'a> u32: sqlx::Type<DB> + sqlx::Decode<'a, DB>,
        for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>,
    {
        Box::pin(Self::count(manager, conds).map_ok(|count| count != 0))
    }
}

pub struct FindOptions<C> {
    pub order_by: Option<OrderBy<C>>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

pub struct OrderBy<C> {
    pub cols: Vec1<C>,
    pub order: Order,
}

pub trait Create<DB: Database>: Entity<DB> + Send {
    type Input: for<'m> From<&'m Self> + ToValues<DB> + Send + Sync;

    fn generated_cols() -> Vec1<&'static str>;

    fn construct(input: &Self::Input, generated: &Record<DB>) -> Result<Self, RecordError>;

    fn create<'m, M: Manager<'m, DB>>(
        manager: M,
        input: Self::Input,
    ) -> BoxFuture<'m, Result<Self, CreateError<M::Error>>> {
        Box::pin(Self::create_many(manager, vec1![input]).map_ok(|mut many| many.pop().unwrap()))
    }

    fn create_many<'m, M: Manager<'m, DB>>(
        manager: M,
        inputs: Vec1<Self::Input>,
    ) -> BoxFuture<'m, Result<Vec<Self>, CreateError<M::Error>>> {
        Box::pin(
            manager
                .insert_returning(InsertReturningQuery {
                    insert_query: InsertQuery {
                        table_name: Self::table_name(),
                        col_names: Self::col_names(),
                        values: inputs
                            .iter()
                            .map(|input| input.to_values())
                            .collect::<Vec<_>>()
                            .try_into()
                            .unwrap(),
                    },
                    returning_cols: Self::generated_cols(),
                })
                .try_collect::<Vec<_>>()
                .map(move |result| match result {
                    Ok(records) => Ok(records
                        .iter()
                        .enumerate()
                        .map(|(index, record)| {
                            Ok::<_, CreateError<_>>(Self::construct(
                                inputs.get(index).ok_or(CreateError::WrongNumberOfRows)?,
                                record,
                            )?)
                        })
                        .collect::<Result<Vec<_>, _>>()?),
                    Err(err) => Err(CreateError::Manager(err)),
                }),
        )
    }

    fn persist<'m, M: Manager<'m, DB>>(&self, manager: M) -> BoxFuture<'m, Result<(), M::Error>> {
        Self::insert(manager, vec1![self.into()])
    }

    fn insert<'m, M: Manager<'m, DB>>(
        manager: M,
        inputs: Vec1<Self::Input>,
    ) -> BoxFuture<'m, Result<(), M::Error>> {
        manager.insert(InsertQuery {
            table_name: Self::table_name(),
            col_names: Self::col_names(),
            values: inputs.mapped(|input| input.to_values()),
        })
    }
}

#[derive(Debug, Error)]
pub enum CreateError<E: Error> {
    #[error(transparent)]
    Manager(E),
    #[error(transparent)]
    Record(#[from] RecordError),
    #[error("number of rows returned by query doesn't match number of inputs")]
    WrongNumberOfRows,
}

pub trait Update<DB: Database>: Entity<DB> + Send {
    type Patch: ToValues<DB> + Send + Sync;

    fn apply_patch(&mut self, patch: Self::Patch);

    fn patch<'e, 'm: 'e, M: Manager<'m, DB>>(
        &'e mut self,
        manager: M,
        patch: Self::Patch,
    ) -> BoxFuture<'e, Result<(), M::Error>> {
        let mut cond = Condition::default();
        cond.add_col(Self::id_col_name(), FindOperator::Eq(Box::new(self.id())));

        Box::pin(
            manager
                .update(UpdateQuery {
                    table_name: Self::table_name(),
                    conds: vec1![cond],
                    new_values: patch.to_values(),
                })
                .and_then(|_| async {
                    self.apply_patch(patch);
                    Ok(())
                }),
        )
    }

    fn update<'m, M: Manager<'m, DB>>(
        manager: M,
        conds: Vec1<Self::Cond>,
        patch: Self::Patch,
    ) -> BoxFuture<'m, Result<(), M::Error>> {
        manager.update(UpdateQuery {
            table_name: Self::table_name(),
            conds: conds.mapped(|cond| cond.into_condition()),
            new_values: patch.to_values(),
        })
    }
}

pub trait Delete<DB: Database>: Entity<DB> {
    fn remove<'m, M: Manager<'m, DB>>(&self, manager: M) -> BoxFuture<'m, Result<(), M::Error>> {
        let mut cond = Condition::default();
        cond.add_col(Self::id_col_name(), FindOperator::Eq(Box::new(self.id())));
        manager.delete(DeleteQuery {
            table_name: Self::table_name(),
            conds: vec1![cond],
        })
    }

    fn delete<'m, M: Manager<'m, DB>>(
        manager: M,
        conds: Vec1<Self::Cond>,
    ) -> BoxFuture<'m, Result<(), M::Error>> {
        manager.delete(DeleteQuery {
            table_name: Self::table_name(),
            conds: conds.mapped(|cond| cond.into_condition()),
        })
    }
}

pub trait Col {
    fn as_str(&self) -> &'static str;
}

pub enum Field<T> {
    Set(T),
    Omit,
}

pub struct Selection<'m, E: Error, T: FromRecord<DB>, DB: Database> {
    stream: BoxStream<'m, Result<Record<DB>, E>>,
    marker: PhantomData<fn() -> T>,
}

#[derive(Debug, Error)]
pub enum SelectError<E: Error> {
    #[error(transparent)]
    Manager(E),
    #[error(transparent)]
    Record(#[from] RecordError),
}

#[derive(Debug, Error)]
pub enum SelectOneError<E: Error> {
    #[error(transparent)]
    Select(#[from] SelectError<E>),
    #[error("query returned less rows than expected")]
    RowNotFound,
}

impl<'m, T: FromRecord<DB>, DB: Database, E: Error + 'm> Selection<'m, E, T, DB> {
    fn new(stream: BoxStream<'m, Result<Record<DB>, E>>) -> Self {
        Self {
            stream,
            marker: PhantomData,
        }
    }

    pub async fn optional(mut self) -> Result<Option<T>, SelectError<E>> {
        self.stream
            .try_next()
            .await
            .map_err(SelectError::Manager)?
            .map(|record| T::from_record(&record).map_err(|e| e.into()))
            .transpose()
    }

    pub async fn one(self) -> Result<T, SelectOneError<E>> {
        self.optional().await?.ok_or(SelectOneError::RowNotFound)
    }

    pub fn stream(self) -> BoxStream<'m, Result<T, SelectError<E>>> {
        Box::pin(self.stream.map(|result| match result {
            Ok(record) => Ok(T::from_record(&record)?),
            Err(err) => Err(SelectError::Manager(err)),
        }))
    }

    pub async fn all(self) -> Result<Vec<T>, SelectError<E>> {
        self.stream().try_collect().await
    }
}
