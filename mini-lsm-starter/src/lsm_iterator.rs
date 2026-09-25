// Copyright (c) 2022-2026 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use anyhow::Result;

use crate::{
    iterators::{StorageIterator, merge_iterator::MergeIterator},
    mem_table::MemTableIterator,
};

/// Represents the internal type for an LSM iterator. This type will be changed across the course for multiple times.
type LsmIteratorInner = MergeIterator<MemTableIterator>;

// LsmIterator 代表存储引擎的内部迭代器
pub struct LsmIterator {
    inner: LsmIteratorInner,
}

impl LsmIterator {
    pub(crate) fn new(iter: LsmIteratorInner) -> Result<Self> {
        let mut lsm_iter = Self { inner: iter };
        // 跳过起点处的墓碑
        while lsm_iter.inner.is_valid() && lsm_iter.inner.value().is_empty() {
            lsm_iter.inner.next()?;
        }
        Ok(lsm_iter)
    }
}

impl StorageIterator for LsmIterator {
    type KeyType<'a> = &'a [u8];

    fn is_valid(&self) -> bool {
        self.inner.is_valid()
    }

    fn key(&self) -> &[u8] {
        self.inner.key().raw_ref()
    }

    fn value(&self) -> &[u8] {
        self.inner.value()
    }

    fn next(&mut self) -> Result<()> {
        if self.inner.is_valid() {
            self.inner.next()?;
        }

        // 当前项是删除标记
        while self.inner.is_valid() && self.inner.value().is_empty() {
            self.inner.next()?;
        }
        Ok(())
    }
}

/// A wrapper around existing iterator, will prevent users from calling `next` when the iterator is
/// invalid. If an iterator is already invalid, `next` does not do anything. If `next` returns an error,
/// `is_valid` should return false, and `next` should always return an error.
pub struct FusedIterator<I: StorageIterator> {
    iter: I,
    has_errored: bool,
}

impl<I: StorageIterator> FusedIterator<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            has_errored: false,
        }
    }
}

impl<I: StorageIterator> StorageIterator for FusedIterator<I> {
    type KeyType<'a>
        = I::KeyType<'a>
    where
        Self: 'a;

    fn is_valid(&self) -> bool {
        if self.has_errored {
            false
        } else {
            self.iter.is_valid()
        }
    }

    fn key(&self) -> Self::KeyType<'_> {
        self.iter.key()
    }

    fn value(&self) -> &[u8] {
        self.iter.value()
    }

    fn next(&mut self) -> Result<()> {
        // 如果发生过错误，next总是报错
        if self.has_errored {
            return Err(anyhow::anyhow!("迭代器此前已发生错误"));
        }
        // 如果已经无效，next不做任何事情
        if !self.is_valid() {
            return Ok(());
        }
        let result = self.iter.next();
        if result.is_err() {
            self.has_errored = true;
        }
        result
    }
}
