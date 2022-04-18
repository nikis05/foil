pub use futures::future::BoxFuture;
use futures::stream::BoxStream;
use sqlx::{Database, Decode, Row, Type};
use std::any::Any;
use std::collections::BTreeMap;
use std::error::Error;
use thiserror::Error;

mod display;
pub mod impls {
    mod executor;
    pub mod log;
    #[cfg(all(feature = "test-manager", feature = "sqlite"))]
    pub mod mock;
}
mod value;

pub use value::Value;

pub use impls::log::LogManager;
#[cfg(all(feature = "test-manager", feature = "sqlite"))]
pub use impls::mock::MockManager;

pub trait Manager<'m, DB: Database>: Send {
    type Error: Error + Send + Sync + 'static;

    fn select<'q, 'o>(
        self,
        query: SelectQuery<'q, DB>,
    ) -> BoxStream<'o, Result<Record<DB>, Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn count<'q, 'o>(self, query: CountQuery<'q, DB>) -> BoxFuture<'o, Result<u32, Self::Error>>
    where
        'm: 'o,
        'q: 'o,
        for<'a> u32: Type<DB> + Decode<'a, DB>,
        for<'a> &'a str: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>;

    fn insert<'q, 'o>(self, query: InsertQuery<'q, DB>) -> BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn insert_returning<'q, 'o>(
        self,
        query: InsertReturningQuery<'q, DB>,
    ) -> BoxStream<'o, Result<Record<DB>, Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn update<'q, 'o>(self, query: UpdateQuery<'q, DB>) -> BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o;

    fn delete<'q, 'o>(self, query: DeleteQuery<'q, DB>) -> BoxFuture<'o, Result<(), Self::Error>>
    where
        'm: 'o,
        'q: 'o;
}

pub struct SelectQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub col_names: &'q [&'q str],
    pub selectors: Vec<Selector<'q, DB>>,
    pub order_by: Option<OrderBy<&'q str>>,
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

pub struct CountQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub selectors: Vec<Selector<'q, DB>>,
}

pub struct InsertQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub values: Vec<InputRecord<'q, DB>>,
}

pub struct InsertReturningQuery<'q, DB: Database> {
    pub insert_query: InsertQuery<'q, DB>,
    pub returning_cols: &'q [&'q str],
}

pub struct UpdateQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub selectors: Vec<Selector<'q, DB>>,
    pub new_values: InputRecord<'q, DB>,
}

pub struct DeleteQuery<'q, DB: Database> {
    pub table_name: &'q str,
    pub selectors: Vec<Selector<'q, DB>>,
}

pub struct Selector<'q, DB: Database>(Vec<(&'q str, FindOperator<Box<dyn Value<'q, DB> + 'q>>)>);

impl<'q, DB: Database> Selector<'q, DB> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn add_col(
        &mut self,
        col_name: &'q str,
        operator: FindOperator<Box<dyn Value<'q, DB> + 'q>>,
    ) {
        self.0.push((col_name, operator));
    }

    pub fn remove_col(&mut self, col_name: &str) {
        if let Some(index) = self.0.iter().position(|entry| entry.0 == col_name) {
            self.0.remove(index);
        }
    }

    pub fn has_col(&self, col_name: &str) -> bool {
        self.0.iter().any(|entry| entry.0 == col_name)
    }

    pub fn col(&self, col_name: &str) -> Option<&FindOperator<Box<dyn Value<'q, DB> + 'q>>> {
        self.0.iter().find_map(|entry| {
            if entry.0 == col_name {
                Some(&entry.1)
            } else {
                None
            }
        })
    }

    pub fn cols(
        &self,
    ) -> impl ExactSizeIterator<Item = (&'q str, &FindOperator<Box<dyn Value<'q, DB> + 'q>>)> {
        self.0
            .iter()
            .map(|(col_name, operator)| (*col_name, operator))
    }

    pub fn into_cols(
        self,
    ) -> impl ExactSizeIterator<Item = (&'q str, FindOperator<Box<dyn Value<'q, DB> + 'q>>)> {
        self.0.into_iter()
    }
}

impl<'q, DB: Database> Default for Selector<'q, DB> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait IntoSelector<'q, DB: Database> {
    fn into_selector(self) -> Selector<'q, DB>;
}

pub enum FindOperator<T> {
    Eq(T),
    Ne(T),
    In(Vec<T>),
    NotIn(Vec<T>),
}

impl<T> FindOperator<T> {
    pub fn boxed<'q, DB: Database>(self) -> FindOperator<Box<dyn Value<'q, DB> + 'q>>
    where
        T: Value<'q, DB> + 'q,
    {
        match self {
            Self::Eq(val) => FindOperator::Eq(Box::new(val)),
            Self::Ne(val) => FindOperator::Ne(Box::new(val)),
            Self::In(vals) => FindOperator::In(
                vals.into_iter()
                    .map(|val| Box::new(val) as Box<dyn Value<'q, _>>)
                    .collect(),
            ),
            Self::NotIn(vals) => FindOperator::NotIn(
                vals.into_iter()
                    .map(|val| Box::new(val) as Box<dyn Value<'q, _>>)
                    .collect(),
            ),
        }
    }
}

pub struct InputRecord<'q, DB: Database>(Vec<(&'q str, Box<dyn Value<'q, DB> + 'q>)>);

impl<'q, DB: Database> InputRecord<'q, DB> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn has_col(&self, col_name: &str) -> bool {
        self.0.iter().any(|entry| entry.0 == col_name)
    }

    pub fn add_col(&mut self, col_name: &'q str, value: Box<dyn Value<'q, DB> + 'q>) {
        self.0.push((col_name, value));
    }

    pub fn remove_col(&mut self, col_name: &str) {
        if let Some(index) = self.0.iter().position(|entry| entry.0 == col_name) {
            self.0.remove(index);
        }
    }

    pub fn col(&self, col_name: &str) -> Option<&Box<dyn Value<'q, DB> + 'q>> {
        self.0.iter().find_map(|entry| {
            if entry.0 == col_name {
                Some(&entry.1)
            } else {
                None
            }
        })
    }

    pub fn cols(&self) -> impl ExactSizeIterator<Item = (&'q str, &Box<dyn Value<'q, DB> + 'q>)> {
        self.0.iter().map(|(col_name, value)| (*col_name, value))
    }

    pub fn into_cols(
        self,
    ) -> impl ExactSizeIterator<Item = (&'q str, Box<dyn Value<'q, DB> + 'q>)> {
        self.0
            .into_iter()
            .map(|(col_name, value)| (col_name, value))
    }
}

impl<'q, DB: Database> Default for InputRecord<'q, DB> {
    fn default() -> Self {
        Self::new()
    }
}

pub trait ToInputRecord<'q, DB: Database> {
    fn to_input_record(&self) -> InputRecord<'q, DB>;
}

pub struct OrderBy<C> {
    pub order: Order,
    pub cols: Vec<C>,
}

pub enum Order {
    Asc,
    Desc,
}

pub struct Record<DB: Database> {
    row: Option<DB::Row>,
    map: BTreeMap<String, Box<dyn Any + Send>>,
}

impl<DB: Database> Record<DB> {
    pub fn new() -> Self {
        Self {
            row: None,
            map: BTreeMap::new(),
        }
    }

    pub fn from_row(row: DB::Row) -> Self {
        Self {
            row: Some(row),
            map: BTreeMap::new(),
        }
    }

    pub fn col<T: sqlx::Type<DB> + for<'d> sqlx::Decode<'d, DB> + Clone + Any>(
        &self,
        col_name: &str,
    ) -> Result<T, RecordError>
    where
        for<'a> &'a str: sqlx::ColumnIndex<DB::Row>,
    {
        if let Some(entry) = self.map.get(col_name) {
            entry
                .as_ref()
                .downcast_ref::<T>()
                .cloned()
                .ok_or_else(|| RecordError::ColumnDecode {
                    index: col_name.into(),
                    source: None,
                })
        } else if let Some(row) = self.row.as_ref() {
            row.try_get::<T, _>(col_name).map_err(|e| match e {
                sqlx::Error::ColumnNotFound(c) => RecordError::ColumnNotFound(c),
                sqlx::Error::ColumnDecode { index, source } => RecordError::ColumnDecode {
                    index,
                    source: Some(source),
                },
                _ => unreachable!(),
            })
        } else {
            Err(RecordError::ColumnNotFound(col_name.into()))
        }
    }
}

impl<DB: Database> Default for Record<DB> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("column not found: {0}")]
    ColumnNotFound(String),
    #[error("error decoding column {index}: {source:?}")]
    ColumnDecode {
        index: String,
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

pub trait FromRecord<DB: Database>: Sized {
    fn from_record(record: &Record<DB>) -> Result<Self, RecordError>;
}
