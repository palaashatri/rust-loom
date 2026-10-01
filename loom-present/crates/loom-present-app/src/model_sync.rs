//! Updating a model's rows without replacing the model.
//!
//! The slide canvas draws one item per row of its models. Replacing a model
//! rebuilds every item, which ends a drag in progress because the item holding
//! the pointer no longer exists. Changing rows in place keeps the items alive.

use slint::{Model, ModelRc, VecModel};

/// `current` with its rows set to `rows`. When `current` is a plain model of the
/// same length, only the rows that differ are written and the same model comes
/// back; otherwise a new model holds the rows.
pub(crate) fn synced<T: Clone + PartialEq + 'static>(
    current: ModelRc<T>,
    rows: Vec<T>,
) -> ModelRc<T> {
    if let Some(model) = current.as_any().downcast_ref::<VecModel<T>>() {
        if model.row_count() == rows.len() {
            for (index, row) in rows.into_iter().enumerate() {
                if model.row_data(index).as_ref() != Some(&row) {
                    model.set_row_data(index, row);
                }
            }
            return current;
        }
    }
    ModelRc::new(VecModel::from(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_length_updates_rows_in_place_and_keeps_the_model() {
        let first = ModelRc::new(VecModel::from(vec![1.0_f32, 2.0, 3.0]));
        let second = synced(first.clone(), vec![1.0, 9.0, 3.0]);
        assert!(std::ptr::eq(
            first.as_any() as *const _ as *const u8,
            second.as_any() as *const _ as *const u8
        ));
        assert_eq!(second.iter().collect::<Vec<_>>(), vec![1.0, 9.0, 3.0]);
    }

    #[test]
    fn a_different_length_gets_a_new_model() {
        let first = ModelRc::new(VecModel::from(vec![1, 2]));
        let second = synced(first.clone(), vec![1, 2, 3]);
        assert!(!std::ptr::eq(
            first.as_any() as *const _ as *const u8,
            second.as_any() as *const _ as *const u8
        ));
        assert_eq!(second.row_count(), 3);
    }

    #[test]
    fn a_model_that_is_not_a_vec_model_is_replaced() {
        let current: ModelRc<i32> = ModelRc::default();
        assert_eq!(synced(current, vec![4]).row_data(0), Some(4));
    }
}
