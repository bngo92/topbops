use std::{
    io::{Read, Seek},
    sync::Arc,
};

use arrow::{
    array::{ArrayRef, BooleanArray},
    compute,
    datatypes::{Fields, Schema},
    ipc::reader::FileReader,
};

#[derive(Clone, Debug)]
pub struct DataFrame {
    pub schema: Arc<Schema>,
    pub arrays: Vec<ArrayRef>,
}

impl DataFrame {
    pub fn from<T: Read + Seek>(buf: &mut T) -> DataFrame {
        let mut reader = FileReader::try_new(buf, None).unwrap();
        DataFrame {
            schema: reader.schema(),
            arrays: reader.next().unwrap().unwrap().columns().to_vec(),
        }
    }

    pub fn column(&self, s: &str) -> Option<&ArrayRef> {
        if let Some(i) = self.schema.fields.iter().position(|f| f.name() == s) {
            self.arrays.get(i)
        } else {
            None
        }
    }

    pub fn drop_in_place(&mut self, s: &str) -> Option<ArrayRef> {
        if let Some(i) = self.schema.fields.iter().position(|f| f.name() == s) {
            self.schema = Arc::new(Schema::new(Fields::from_iter(
                self.schema
                    .fields
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i1, f)| if i == i1 { Some(Arc::clone(f)) } else { None }),
            )));
            Some(self.arrays.remove(i))
        } else {
            None
        }
    }

    pub fn remove(&mut self, mut index: Vec<bool>) {
        index.sort();
        index.reverse();
        let predicate = BooleanArray::from(index);
        for a in &mut self.arrays {
            *a = compute::filter(a, &predicate).unwrap();
        }
    }
}
