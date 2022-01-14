use super::{
    Condition, CountQuery, DeleteQuery, FindOperator, InsertQuery, InsertReturningQuery, Order,
    SelectQuery, UpdateQuery, Value,
};
use sqlx::Database;
use std::{
    collections::BTreeSet,
    fmt::{Display, Formatter, Result},
};

#[cfg(all(test, feature = "postgres", feature = "sqlite", feature = "mssql"))]
mod test;

pub trait WithBindParameters {
    type ParameterFactory: ParameterFactory;

    fn parameter_factory() -> Self::ParameterFactory {
        Self::ParameterFactory::default()
    }
}

pub trait ParameterFactory: Default {
    fn get(&mut self) -> String;
}

#[cfg(feature = "postgres")]
mod pg_parameters {
    use super::{ParameterFactory, WithBindParameters};

    impl WithBindParameters for sqlx::Postgres {
        type ParameterFactory = PgParameterFactory;
    }

    #[derive(Default)]
    pub struct PgParameterFactory(usize);

    impl ParameterFactory for PgParameterFactory {
        fn get(&mut self) -> String {
            self.0 += 1;
            format!("${}", self.0)
        }
    }
}

#[cfg(any(feature = "mysql", feature = "sqlite", feature = "any"))]
mod unordered_parameters {
    use super::{ParameterFactory, WithBindParameters};

    #[cfg(feature = "mysql")]
    impl WithBindParameters for sqlx::MySql {
        type ParameterFactory = UnorderedParameterFactory;
    }

    #[cfg(feature = "sqlite")]
    impl WithBindParameters for sqlx::Sqlite {
        type ParameterFactory = UnorderedParameterFactory;
    }

    #[cfg(feature = "any")]
    impl WithBindParameters for sqlx::Any {
        type ParameterFactory = UnorderedParameterFactory;
    }

    #[derive(Default)]
    pub struct UnorderedParameterFactory;

    impl ParameterFactory for UnorderedParameterFactory {
        fn get(&mut self) -> String {
            "?".into()
        }
    }
}

#[cfg(feature = "mssql")]
mod mssql_parameters {
    use super::{ParameterFactory, WithBindParameters};

    impl WithBindParameters for sqlx::Mssql {
        type ParameterFactory = MssqlParameterFactory;
    }

    #[derive(Default)]
    pub struct MssqlParameterFactory(usize);

    impl ParameterFactory for MssqlParameterFactory {
        fn get(&mut self) -> String {
            self.0 += 1;
            format!("@P{}", self.0)
        }
    }
}

impl<DB: Database + WithBindParameters> Display for SelectQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "SELECT ")?;

        format_list(self.col_names.iter(), f, |col_name, f| {
            write!(f, "\"{}\"", col_name)
        })?;

        write!(f, " FROM \"{}\"", self.table_name)?;

        format_conds(&self.conds, &mut DB::parameter_factory(), f)?;

        if let Some((order_by_cols, order)) = &self.order_by {
            write!(f, " ORDER BY ")?;

            format_list(order_by_cols.iter(), f, |col_name, f| {
                write!(f, "\"{}\"", col_name)
            })?;

            write!(f, " {}", order)?;
        }

        if let Some(offset) = self.offset {
            write!(f, " SKIP {}", offset)?;
        }

        if let Some(limit) = self.limit {
            write!(f, " TAKE {}", limit)?;
        }

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for CountQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "SELECT COUNT (*) AS \"cnt\" FROM \"{}\"",
            self.table_name
        )?;

        format_conds(&self.conds, &mut DB::parameter_factory(), f)?;

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for InsertQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "INSERT INTO \"{}\" (", self.table_name)?;

        format_list(self.col_names.iter(), f, |col_name, f| {
            write!(f, "\"{}\"", col_name)
        })?;

        write!(f, ") VALUES ")?;

        let mut parameter_factory = DB::parameter_factory();

        format_list(self.values.iter(), f, |values, f| {
            write!(f, "(")?;

            format_list(self.col_names.iter(), f, |col_name, f| {
                if values.has_col(col_name) {
                    write!(f, "{}", parameter_factory.get())?;
                } else {
                    write!(f, "DEFAULT")?;
                }

                Ok(())
            })?;

            write!(f, ")")?;

            Ok(())
        })?;

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for InsertReturningQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.insert_query)?;

        if self.returning_cols.is_empty() {
            return Ok(());
        }

        write!(f, " RETURNING ")?;

        format_list(self.returning_cols.iter(), f, |col_name, f| {
            write!(f, "\"{}\"", col_name)
        })?;
        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for UpdateQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.new_values.is_empty() {
            return write!(f, "<UPDATE query with no new values>");
        }

        write!(f, "UPDATE \"{}\" SET ", self.table_name)?;

        let mut parameter_factory = DB::parameter_factory();
        let cols = self.new_values.cols();
        let len = cols.len();

        for (index, (col_name, _)) in cols.enumerate() {
            write!(f, "\"{}\" = {}", col_name, parameter_factory.get())?;

            if index != len - 1 {
                write!(f, ", ")?;
            }
        }

        format_conds(&self.conds, &mut parameter_factory, f)?;

        Ok(())
    }
}

impl<DB: Database + WithBindParameters> Display for DeleteQuery<'_, DB> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "DELETE FROM \"{}\"", self.table_name)?;
        format_conds(&self.conds, &mut DB::parameter_factory(), f)
    }
}

impl Display for Order {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        f.write_str(match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        })
    }
}

fn format_conds<DB: Database + WithBindParameters>(
    conds: &[Condition<DB>],
    parameter_factory: &mut DB::ParameterFactory,
    w: &mut impl std::fmt::Write,
) -> Result {
    if conds.iter().any(Condition::is_empty) {
        return Ok(());
    }

    write!(w, " WHERE ")?;

    if conds.len() > 1 {
        for (index, cond) in conds.iter().enumerate() {
            write!(w, "(")?;
            format_cond(cond, parameter_factory, w)?;
            write!(w, ")")?;

            if index != conds.len() - 1 {
                write!(w, " OR ")?;
            }
        }
    } else {
        format_cond(conds.first().unwrap(), parameter_factory, w)?;
    }

    Ok(())
}

fn format_cond<DB: Database + WithBindParameters, W: std::fmt::Write>(
    cond: &Condition<DB>,
    parameter_factory: &mut DB::ParameterFactory,
    f: &mut W,
) -> Result {
    let mut format_col =
        |f: &mut W, (col_name, op): (&str, &FindOperator<Box<dyn Value<DB>>>)| match op {
            FindOperator::Eq(value) => {
                if value.is_null() {
                    write!(f, "\"{}\" IS NULL", col_name)
                } else {
                    write!(f, "\"{}\" = {}", col_name, parameter_factory.get())
                }
            }
            FindOperator::Ne(value) => {
                if value.is_null() {
                    write!(f, "\"{}\" IS NOT NULL", col_name)
                } else {
                    write!(f, "\"{}\" != {}", col_name, parameter_factory.get())
                }
            }
            FindOperator::In(values) => {
                write!(f, "\"{}\" IN (", col_name)?;

                format_list(
                    values
                        .iter()
                        .filter_map(|value| {
                            if value.is_null() {
                                None
                            } else {
                                Some(parameter_factory.get())
                            }
                        })
                        .collect::<Vec<_>>()
                        .iter(),
                    f,
                    |parameter, f| write!(f, "{}", parameter),
                )?;

                write!(f, ")")?;
                if values.iter().any(|value| value.is_null()) {
                    write!(f, " OR \"{}\" IS NULL", col_name)?;
                }

                Ok(())
            }
            FindOperator::NotIn(values) => {
                write!(f, "\"{}\" NOT IN (", col_name)?;

                format_list(
                    values
                        .iter()
                        .filter_map(|value| {
                            if value.is_null() {
                                None
                            } else {
                                Some(parameter_factory.get())
                            }
                        })
                        .collect::<Vec<_>>()
                        .iter(),
                    f,
                    |parameter, f| write!(f, "{}", parameter),
                )?;

                write!(f, ")")?;

                if values.iter().any(|value| value.is_null()) {
                    write!(f, " AND \"{}\" IS NOT NULL", col_name)?;
                }

                Ok(())
            }
        };

    if cond.len() == 1 {
        format_col(f, cond.cols().next().unwrap())?;
    } else {
        for (index, col) in cond.cols().enumerate() {
            write!(f, "(")?;
            format_col(f, col)?;
            write!(f, ")")?;

            if index != cond.cols().len() - 1 {
                write!(f, " AND ")?;
            }
        }
    }

    Ok(())
}

fn format_list<T, W: std::fmt::Write>(
    list: impl ExactSizeIterator<Item = T>,
    f: &mut W,
    mut format_fn: impl FnMut(T, &mut W) -> Result,
) -> Result {
    let len = list.len();
    for (index, item) in list.enumerate() {
        format_fn(item, f)?;
        if index != len - 1 {
            write!(f, ", ")?;
        }
    }

    Ok(())
}
