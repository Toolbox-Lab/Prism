use stellar_xdr::curr::ScVal;

#[derive(Debug, Clone, PartialEq)]
pub enum ScValPathSegment {
    MapKey(ScVal),
    VecIndex(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScValChange {
    pub path: Vec<ScValPathSegment>,
    pub old: Option<ScVal>,
    pub new: Option<ScVal>,
}

pub fn compare_scval(old: &ScVal, new: &ScVal) -> Option<Vec<ScValChange>> {
    let mut changes = Vec::new();
    compare_at(old, new, &mut Vec::new(), &mut changes);
    (!changes.is_empty()).then_some(changes)
}

fn compare_at(
    old: &ScVal,
    new: &ScVal,
    path: &mut Vec<ScValPathSegment>,
    changes: &mut Vec<ScValChange>,
) {
    if old == new {
        return;
    }

    match (old, new) {
        (ScVal::Map(Some(old_map)), ScVal::Map(Some(new_map))) => {
            for old_entry in old_map.iter() {
                path.push(ScValPathSegment::MapKey(old_entry.key.clone()));
                if let Some(new_entry) = new_map.iter().find(|entry| entry.key == old_entry.key) {
                    compare_at(&old_entry.val, &new_entry.val, path, changes);
                } else {
                    changes.push(ScValChange {
                        path: path.clone(),
                        old: Some(old_entry.val.clone()),
                        new: None,
                    });
                }
                path.pop();
            }

            for new_entry in new_map.iter() {
                if !old_map.iter().any(|entry| entry.key == new_entry.key) {
                    path.push(ScValPathSegment::MapKey(new_entry.key.clone()));
                    changes.push(ScValChange {
                        path: path.clone(),
                        old: None,
                        new: Some(new_entry.val.clone()),
                    });
                    path.pop();
                }
            }
        }
        (ScVal::Vec(Some(old_vec)), ScVal::Vec(Some(new_vec))) => {
            for index in 0..old_vec.len().max(new_vec.len()) {
                path.push(ScValPathSegment::VecIndex(index));
                match (old_vec.get(index), new_vec.get(index)) {
                    (Some(old_value), Some(new_value)) => {
                        compare_at(old_value, new_value, path, changes);
                    }
                    (Some(old_value), None) => changes.push(ScValChange {
                        path: path.clone(),
                        old: Some(old_value.clone()),
                        new: None,
                    }),
                    (None, Some(new_value)) => changes.push(ScValChange {
                        path: path.clone(),
                        old: None,
                        new: Some(new_value.clone()),
                    }),
                    (None, None) => unreachable!(),
                }
                path.pop();
            }
        }
        _ => changes.push(ScValChange {
            path: path.clone(),
            old: Some(old.clone()),
            new: Some(new.clone()),
        }),
    }
}
