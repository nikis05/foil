use crate::manager::{
    CountQuery, DeleteQuery, FindOperator, FromRecord, InsertQuery, InsertReturningQuery,
    IntoSelector, Manager, OrderBy, Record, RecordError, SelectQuery, Selector, ToInputRecord,
    UpdateQuery, Value,
};
use futures::{
    future::BoxFuture, stream::BoxStream, FutureExt, StreamExt, TryFutureExt, TryStreamExt,
};
use sqlx::Database;
use std::error::Error;
use std::marker::PhantomData;
use thiserror::Error;

#[cfg(all(
    test,
    feature = "test-manager",
    feature = "runtime-tokio-rustls",
    feature = "uuid",
    feature = "tokio"
))]
mod test;

// #[derive(Entity, Create)]
// struct Character {
//     id: u8,
//     name: String,
//     is_handsome: bool,
//     #[foil(generated)]
//     father_name: ::std::option::Option<String>,
// }

pub trait Entity<DB: Database>: FromRecord<DB> + 'static {
    type Col: Col + Send;
    type Id: for<'q> Value<'q, DB> + Send;
    type Selector<'q>: IntoSelector<'q, DB> + Default + Send;

    fn table_name() -> &'static str;

    fn col_names() -> &'static [&'static str];

    fn id_col_name() -> &'static str;

    fn id(&self) -> Self::Id;

    fn get<'m, M: Manager<'m, DB>>(
        manager: M,
        id: Self::Id,
    ) -> BoxFuture<'m, Result<Self, SelectOneError<M::Error>>> {
        let mut selector = Selector::new();
        selector.add_col(Self::id_col_name(), FindOperator::Eq(Box::new(id)));

        Box::pin(
            Selection::<_, Self, DB>::new(manager.select(SelectQuery {
                table_name: Self::table_name(),
                col_names: Self::col_names(),
                selectors: vec![selector],
                order_by: None,
                offset: None,
                limit: None,
            }))
            .one(),
        )
    }

    fn find<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        selectors: Vec<Self::Selector<'q>>,
    ) -> Selection<'o, M::Error, Self, DB> {
        Self::find_with_options(
            manager,
            selectors,
            FindOptions {
                order_by: None,
                offset: None,
                limit: None,
            },
        )
    }

    fn find_with_options<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        selectors: Vec<Self::Selector<'q>>,
        options: FindOptions<Self::Col>,
    ) -> Selection<'o, M::Error, Self, DB> {
        Selection::new(
            manager.select(SelectQuery {
                table_name: Self::table_name(),
                col_names: Self::col_names(),
                selectors: selectors
                    .into_iter()
                    .map(IntoSelector::into_selector)
                    .collect(),
                order_by: options.order_by.map(|order_by| OrderBy {
                    order: order_by.order,
                    cols: order_by.cols.iter().map(Col::as_str).collect(),
                }),
                offset: options.offset,
                limit: options.limit,
            }),
        )
    }

    fn count<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        selectors: Vec<Self::Selector<'q>>,
    ) -> BoxFuture<'o, Result<u32, M::Error>>
    where
        for<'a> u32: sqlx::Type<DB> + sqlx::Decode<'a, DB>,
        for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>,
    {
        manager.count(CountQuery {
            table_name: Self::table_name(),
            selectors: selectors
                .into_iter()
                .map(IntoSelector::into_selector)
                .collect(),
        })
    }

    fn exists<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        selectors: Vec<Self::Selector<'q>>,
    ) -> BoxFuture<'o, Result<bool, M::Error>>
    where
        for<'a> u32: sqlx::Type<DB> + sqlx::Decode<'a, DB>,
        for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>,
    {
        Box::pin(Self::count(manager, selectors).map_ok(|count| count != 0))
    }
}

pub struct FindOptions<C> {
    pub order_by: Option<OrderBy<C>>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

pub trait Create<DB: Database>: Entity<DB> + Send {
    type Input<'q>: From<&'q Self> + ToInputRecord<'q, DB> + Send + Sync;

    fn generated_col_names() -> &'static [&'static str];

    fn construct<'q>(input: &Self::Input<'q>, generated: &Record<DB>) -> Result<Self, RecordError>;

    fn create<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        input: Self::Input<'q>,
    ) -> BoxFuture<'o, Result<Self, CreateError<M::Error>>> {
        Box::pin(Self::create_many(manager, vec![input]).map_ok(|mut many| many.pop().unwrap()))
    }

    fn create_many<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        inputs: Vec<Self::Input<'q>>,
    ) -> BoxFuture<'o, Result<Vec<Self>, CreateError<M::Error>>> {
        Box::pin(
            manager
                .insert_returning(InsertReturningQuery {
                    insert_query: InsertQuery {
                        table_name: Self::table_name(),
                        values: inputs
                            .iter()
                            .map(ToInputRecord::to_input_record)
                            .collect::<Vec<_>>(),
                    },
                    returning_cols: Self::generated_col_names(),
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

    fn persist<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        &'q self,
        manager: M,
    ) -> BoxFuture<'o, Result<(), M::Error>> {
        Self::insert(manager, vec![self.into()])
    }

    fn insert<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        inputs: Vec<Self::Input<'q>>,
    ) -> BoxFuture<'o, Result<(), M::Error>> {
        manager.insert(InsertQuery {
            table_name: Self::table_name(),
            values: inputs.iter().map(ToInputRecord::to_input_record).collect(),
        })
    }
}

#[derive(Debug, Error)]
pub enum CreateError<E: Error + Send + Sync> {
    #[error(transparent)]
    Manager(E),
    #[error(transparent)]
    Record(#[from] RecordError),
    #[error("number of rows returned by query doesn't match number of inputs")]
    WrongNumberOfRows,
}

pub trait Update<DB: Database>: Entity<DB> + Send {
    type Patch<'q>: ToInputRecord<'q, DB> + Default + Send + Sync;

    fn apply_patch(&mut self, patch: Self::Patch<'_>);

    fn patch<'m: 'o, 'q: 'o, 'e: 'o, 'o, M: Manager<'m, DB>>(
        &'e mut self,
        manager: M,
        patch: Self::Patch<'q>,
    ) -> BoxFuture<'o, Result<(), M::Error>> {
        let mut selector = Selector::new();
        selector.add_col(
            Self::id_col_name(),
            FindOperator::Eq(Box::new(self.id()) as Box<dyn Value<'q, _>>),
        );

        Box::pin(
            manager
                .update(UpdateQuery {
                    table_name: Self::table_name(),
                    selectors: vec![selector],
                    new_values: patch.to_input_record(),
                })
                .and_then(|_| async {
                    self.apply_patch(patch);
                    Ok(())
                }),
        )
    }

    fn update<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        selectors: Vec<Self::Selector<'q>>,
        patch: Self::Patch<'q>,
    ) -> BoxFuture<'o, Result<(), M::Error>> {
        manager.update(UpdateQuery {
            table_name: Self::table_name(),
            selectors: selectors
                .into_iter()
                .map(IntoSelector::into_selector)
                .collect(),
            new_values: patch.to_input_record(),
        })
    }
}

pub trait Delete<DB: Database>: Entity<DB> {
    fn remove<'m, M: Manager<'m, DB>>(&self, manager: M) -> BoxFuture<'m, Result<(), M::Error>> {
        let mut selector = Selector::new();
        selector.add_col(Self::id_col_name(), FindOperator::Eq(Box::new(self.id())));
        manager.delete(DeleteQuery {
            table_name: Self::table_name(),
            selectors: vec![selector],
        })
    }

    fn delete<'m: 'o, 'q: 'o, 'o, M: Manager<'m, DB>>(
        manager: M,
        selectors: Vec<Self::Selector<'q>>,
    ) -> BoxFuture<'o, Result<(), M::Error>> {
        manager.delete(DeleteQuery {
            table_name: Self::table_name(),
            selectors: selectors
                .into_iter()
                .map(IntoSelector::into_selector)
                .collect(),
        })
    }
}

pub trait Col: Copy {
    fn as_str(&self) -> &'static str;
}

pub enum Field<T> {
    Set(T),
    Omit,
}

impl<T> Default for Field<T> {
    fn default() -> Self {
        Self::Omit
    }
}

pub struct Selection<'m, E: Error + Send + Sync, T: FromRecord<DB>, DB: Database> {
    stream: BoxStream<'m, Result<Record<DB>, E>>,
    marker: PhantomData<fn() -> T>,
}

#[derive(Debug, Error)]
pub enum SelectError<E: Error + Send + Sync> {
    #[error(transparent)]
    Manager(E),
    #[error(transparent)]
    Record(#[from] RecordError),
}

#[derive(Debug, Error)]
pub enum SelectOneError<E: Error + Send + Sync> {
    #[error(transparent)]
    Select(#[from] SelectError<E>),
    #[error("query returned less rows than expected")]
    RowNotFound,
}

impl<'m, T: FromRecord<DB>, DB: Database, E: Error + Send + Sync + 'm> Selection<'m, E, T, DB> {
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
            .map(|record| T::from_record(&record).map_err(Into::into))
            .transpose()
    }

    pub async fn one(self) -> Result<T, SelectOneError<E>> {
        self.optional().await?.ok_or(SelectOneError::RowNotFound)
    }

    #[allow(clippy::must_use_candidate)]
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
