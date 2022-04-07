use crate::manager::{
    display::{format_selectors, WithBindParameters},
    CountQuery, DeleteQuery, FindOperator, InputRecord, InsertQuery, InsertReturningQuery, Order,
    OrderBy, SelectQuery, Selector, UpdateQuery, Value,
};
use insta::assert_snapshot;
use sqlx::{Mssql, Postgres, Sqlite};

mod selectors {
    use super::*;

    #[test]
    fn no_selectors() {
        let mut output = String::new();

        format_selectors::<Postgres, _>(&[], &mut Postgres::parameter_factory(), &mut output)
            .unwrap();

        assert_snapshot!(output, @" WHERE <empty list>");
    }

    #[test]
    fn single_empty_selector() {
        let mut output = String::new();

        let cond = Selector::<Postgres>::default();

        format_selectors(&[cond], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @"");
    }

    #[test]
    fn empty_selector() {
        let mut output = String::new();

        let mut cond1 = Selector::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));
        let cond2 = Selector::<Postgres>::default();

        format_selectors(
            &[cond1, cond2],
            &mut Postgres::parameter_factory(),
            &mut output,
        )
        .unwrap();

        assert_snapshot!(output, @r###" WHERE ("col1" = $1) OR (<empty list>)"###);
    }

    #[test]
    fn eq() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col("col", FindOperator::Eq(Box::new(1)));

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" = $1"###);
    }

    #[test]
    fn eq_null() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col("col", FindOperator::Eq(Box::new(Option::<i32>::None)));

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IS NULL"###);
    }

    #[test]
    fn ne() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col("col", FindOperator::Ne(Box::new(1)));

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" != $1"###);
    }

    #[test]
    fn ne_null() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col("col", FindOperator::Ne(Box::new(Option::<i32>::None)));

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IS NOT NULL"###);
    }

    #[test]
    fn in_() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col(
            "col",
            FindOperator::In(vec![Box::new(1), Box::new(2), Box::new(3)]),
        );

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN ($1, $2, $3)"###);
    }

    #[test]
    fn in_with_null() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col(
            "col",
            FindOperator::In(vec![
                Box::new(Some(1)),
                Box::new(Some(2)),
                Box::new(Option::<i32>::None),
                Box::new(Option::<i32>::None),
            ]),
        );

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN ($1, $2) OR "col" IS NULL"###);
    }

    #[test]
    fn not_in() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col(
            "col",
            FindOperator::NotIn(vec![Box::new(1), Box::new(2), Box::new(3)]),
        );

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" NOT IN ($1, $2, $3)"###);
    }

    #[test]
    fn not_in_with_null() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col(
            "col",
            FindOperator::NotIn(vec![
                Box::new(Some(1)),
                Box::new(Some(2)),
                Box::new(Option::<i32>::None),
                Box::new(Option::<i32>::None),
            ]),
        );

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" NOT IN ($1, $2) AND "col" IS NOT NULL"###);
    }

    #[test]
    fn multiple_cols() {
        let mut output = String::new();

        let mut selector = Selector::<Postgres>::default();
        selector.add_col("col1", FindOperator::Eq(Box::new(1)));
        selector.add_col("col2", FindOperator::Eq(Box::new(2)));

        format_selectors(&[selector], &mut Postgres::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE ("col1" = $1) AND ("col2" = $2)"###);
    }

    #[test]
    fn multiple_selectors() {
        let mut output = String::new();

        let mut cond1 = Selector::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));

        let mut cond2 = Selector::default();
        cond2.add_col("col2", FindOperator::Eq(Box::new(2)));

        format_selectors(
            &[cond1, cond2],
            &mut Postgres::parameter_factory(),
            &mut output,
        )
        .unwrap();

        assert_snapshot!(output, @r###" WHERE ("col1" = $1) OR ("col2" = $2)"###);
    }

    #[test]
    fn sqlite_mysql_params() {
        let mut output = String::new();

        let mut selector = Selector::<Sqlite>::default();
        selector.add_col(
            "col",
            FindOperator::In(vec![
                Box::new(1) as Box<dyn Value<Sqlite>>,
                Box::new(2),
                Box::new(3),
            ]),
        );

        format_selectors(&[selector], &mut Sqlite::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN (?, ?, ?)"###);
    }

    #[test]
    fn mssql_params() {
        let mut output = String::new();

        let mut selector = Selector::<Mssql>::default();
        selector.add_col(
            "col",
            FindOperator::In(vec![
                Box::new(1) as Box<dyn Value<Mssql>>,
                Box::new(2),
                Box::new(3),
            ]),
        );

        format_selectors(&[selector], &mut Mssql::parameter_factory(), &mut output).unwrap();

        assert_snapshot!(output, @r###" WHERE "col" IN (@P1, @P2, @P3)"###);
    }

    #[test]
    fn combined() {
        let mut output = String::new();

        let mut cond1 = Selector::<Postgres>::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));
        cond1.add_col("col2", FindOperator::Eq(Box::new(Option::<i32>::None)));
        cond1.add_col(
            "col3",
            FindOperator::In(vec![Box::new(2), Box::new(Option::<i32>::None)]),
        );

        let mut cond2 = Selector::<Postgres>::default();
        cond2.add_col("col1", FindOperator::Ne(Box::new(1)));
        cond2.add_col("col2", FindOperator::Ne(Box::new(Option::<i32>::None)));
        cond2.add_col(
            "col3",
            FindOperator::NotIn(vec![Box::new(2), Box::new(Option::<i32>::None)]),
        );

        format_selectors(
            &[cond1, cond2],
            &mut Postgres::parameter_factory(),
            &mut output,
        )
        .unwrap();

        assert_snapshot!(output, @r###" WHERE (("col1" = $1) AND ("col2" IS NULL) AND ("col3" IN ($2) OR "col3" IS NULL)) OR (("col1" != $3) AND ("col2" IS NOT NULL) AND ("col3" NOT IN ($4) AND "col3" IS NOT NULL))"###);
    }
}

mod select_query {
    use super::*;

    #[test]
    fn normal() {
        let mut selector = Selector::default();
        selector.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery::<Postgres> {
            table_name: "table",
            col_names: &["col1", "col2"],
            selectors: vec![selector],
            order_by: None,
            offset: None,
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1"###);
    }

    #[test]
    fn no_cols() {
        let mut selector = Selector::default();
        selector.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery::<Postgres> {
            table_name: "table",
            col_names: &[],
            selectors: vec![selector],
            order_by: None,
            offset: None,
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT <empty list> FROM "table" WHERE "col1" = $1"###);
    }

    #[test]
    fn order_by() {
        let mut selector = Selector::default();
        selector.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery::<Postgres> {
            table_name: "table",
            col_names: &["col1", "col2"],
            selectors: vec![selector],
            order_by: Some(OrderBy {
                order: Order::Asc,
                cols: vec!["col1", "col2"],
            }),
            offset: None,
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1 ORDER BY "col1", "col2" ASC"###);
    }

    #[test]
    fn skip() {
        let mut selector = Selector::default();
        selector.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery::<Postgres> {
            table_name: "table",
            col_names: &["col1", "col2"],
            selectors: vec![selector],
            order_by: None,
            offset: Some(3),
            limit: None,
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1 SKIP 3"###);
    }

    #[test]
    fn take() {
        let mut selector = Selector::default();
        selector.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = SelectQuery::<Postgres> {
            table_name: "table",
            col_names: &["col1", "col2"],
            selectors: vec![selector],
            order_by: None,
            offset: None,
            limit: Some(3),
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE "col1" = $1 TAKE 3"###);
    }

    #[test]
    fn combined() {
        let mut cond1 = Selector::default();
        cond1.add_col("col1", FindOperator::Eq(Box::new(1)));
        cond1.add_col("col2", FindOperator::Eq(Box::new(Option::<i32>::None)));
        cond1.add_col(
            "col3",
            FindOperator::In(vec![Box::new(2), Box::new(Option::<i32>::None)]),
        );

        let mut cond2 = Selector::default();
        cond2.add_col("col1", FindOperator::Ne(Box::new(1)));
        cond2.add_col("col2", FindOperator::Ne(Box::new(Option::<i32>::None)));
        cond2.add_col(
            "col3",
            FindOperator::NotIn(vec![Box::new(2), Box::new(Option::<i32>::None)]),
        );

        let query = SelectQuery::<Postgres> {
            table_name: "table",
            col_names: &["col1", "col2"],
            selectors: vec![cond1, cond2],
            order_by: Some(OrderBy {
                order: Order::Asc,
                cols: vec!["col1", "col2"],
            }),
            offset: Some(3),
            limit: Some(5),
        };

        assert_snapshot!(query.to_string(), @r###"SELECT "col1", "col2" FROM "table" WHERE (("col1" = $1) AND ("col2" IS NULL) AND ("col3" IN ($2) OR "col3" IS NULL)) OR (("col1" != $3) AND ("col2" IS NOT NULL) AND ("col3" NOT IN ($4) AND "col3" IS NOT NULL)) ORDER BY "col1", "col2" ASC SKIP 3 TAKE 5"###);
    }
}

mod count_query {
    use super::*;

    #[test]
    fn normal() {
        let mut selector = Selector::default();
        selector.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = CountQuery::<Postgres> {
            table_name: "table",
            selectors: vec![selector],
        };

        assert_snapshot!(query.to_string(), @r###"SELECT COUNT (*) AS "cnt" FROM "table" WHERE "col1" = $1"###);
    }
}

mod insert_query {
    use super::*;

    #[test]
    fn normal() {
        let mut values = InputRecord::default();
        values.add_col("col1", Box::new(1));

        let query = InsertQuery::<Postgres> {
            table_name: "table",
            values: vec![values],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1") VALUES ($1)"###);
    }

    #[test]
    fn no_cols() {
        let mut values = InputRecord::default();
        values.add_col("col1", Box::new(1));

        let query = InsertQuery::<Postgres> {
            table_name: "table",
            values: vec![values],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1") VALUES ($1)"###);
    }

    #[test]
    fn no_values() {
        let query = InsertQuery::<Postgres> {
            table_name: "table",
            values: vec![],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" (<empty list>) VALUES <empty list>"###);
    }

    #[test]
    fn multiple_values() {
        let mut values1 = InputRecord::default();
        values1.add_col("col1", Box::new(1));
        values1.add_col("col2", Box::new(1));

        let mut values2 = InputRecord::default();
        values2.add_col("col1", Box::new(1));
        values2.add_col("col2", Box::new(1));

        let query = InsertQuery::<Postgres> {
            table_name: "table",
            values: vec![values1, values2],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1", "col2") VALUES ($1, $2), ($3, $4)"###);
    }

    #[test]
    fn values_with_different_column_sets() {
        let mut values1 = InputRecord::default();
        values1.add_col("col1", Box::new(1));
        values1.add_col("col2", Box::new(1));

        let mut values2 = InputRecord::default();
        values2.add_col("col2", Box::new(1));
        values2.add_col("col3", Box::new(1));

        let values3 = InputRecord::default();

        let query = InsertQuery::<Postgres> {
            table_name: "table",
            values: vec![values1, values2, values3],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1", "col2", "col3") VALUES ($1, $2, DEFAULT), (DEFAULT, $3, $4), (DEFAULT, DEFAULT, DEFAULT)"###);
    }
}

mod insert_returning_query {
    use super::*;

    #[test]
    fn normal() {
        let mut values = InputRecord::default();
        values.add_col("col1", Box::new(1));

        let query = InsertReturningQuery::<Postgres> {
            insert_query: InsertQuery {
                table_name: "table",
                values: vec![values],
            },
            returning_cols: &["col1", "col2"],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1") VALUES ($1) RETURNING "col1", "col2""###);
    }

    #[test]
    fn no_returning() {
        let mut values = InputRecord::default();
        values.add_col("col1", Box::new(1));

        let query = InsertReturningQuery::<Postgres> {
            insert_query: InsertQuery {
                table_name: "table",
                values: vec![values],
            },
            returning_cols: &[],
        };

        assert_snapshot!(query.to_string(), @r###"INSERT INTO "table" ("col1") VALUES ($1)"###);
    }
}

mod update_query {
    use super::*;

    #[test]
    fn normal() {
        let mut selectors = Selector::default();
        selectors.add_col("col1", FindOperator::Eq(Box::new(1)));

        let mut values = InputRecord::default();
        values.add_col("col2", Box::new(2));

        let query = UpdateQuery::<Postgres> {
            table_name: "table",
            selectors: vec![selectors],
            new_values: values,
        };

        assert_snapshot!(query.to_string(), @r###"UPDATE "table" SET "col2" = $1 WHERE "col1" = $2"###);
    }

    #[test]
    fn empty_values() {
        let mut selectors = Selector::default();
        selectors.add_col("col1", FindOperator::Eq(Box::new(1)));

        let values = InputRecord::default();

        let query = UpdateQuery::<Postgres> {
            table_name: "table",
            selectors: vec![selectors],
            new_values: values,
        };

        assert_snapshot!(query.to_string(), @r###"UPDATE "table" SET <empty list> WHERE "col1" = $1"###);
    }
}

mod delete_query {
    use super::*;

    #[test]
    fn normal() {
        let mut selectors = Selector::default();
        selectors.add_col("col1", FindOperator::Eq(Box::new(1)));

        let query = DeleteQuery::<Postgres> {
            table_name: "table",
            selectors: vec![selectors],
        };

        assert_snapshot!(query.to_string(), @r###"DELETE FROM "table" WHERE "col1" = $1"###);
    }
}
