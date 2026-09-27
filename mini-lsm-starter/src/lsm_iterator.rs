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

use crate::{
    iterators::{
        StorageIterator, merge_iterator::MergeIterator, two_merge_iterator::TwoMergeIterator,
    },
    mem_table::MemTableIterator,
    table::SsTableIterator,
};
use anyhow::Result;
use bytes::Bytes;
use std::ops::Bound;

/// Represents the internal type for an LSM iterator. This type will be changed across the course for multiple times.
type LsmIteratorInner =
    TwoMergeIterator<MergeIterator<MemTableIterator>, MergeIterator<SsTableIterator>>;

// LsmIterator 代表存储引擎的内部迭代器
pub struct LsmIterator {
    inner: LsmIteratorInner,
    end: Bound<Bytes>,
    invalid: bool,
}

impl LsmIterator {
    pub(crate) fn new(iter: LsmIteratorInner, end: Bound<Bytes>) -> Result<Self> {
        let mut lsm_iter = Self {
            inner: iter,
            end,
            invalid: false,
        };
        lsm_iter.skip_deleted()?;

        Ok(lsm_iter)
    }

    fn check_out_of_bound(&mut self) -> bool {
        match &self.end {
            Bound::Excluded(end) => {
                if self.inner.is_valid() && self.inner.key().raw_ref() >= end.as_ref() {
                    // 已经越界，设定为无效，直接返回
                    self.invalid = true;
                    return true;
                }
            }
            Bound::Included(end) => {
                if self.inner.is_valid() && self.inner.key().raw_ref() > end.as_ref() {
                    self.invalid = true;
                    return true;
                }
            }
            Bound::Unbounded => {
                // 没有边界，什么都不干
            }
        }
        false
    }

    // 当前 key 已是最后一个可能输出的 key；消费它或跳过它时无需再读取后面的块。
    fn reached_end(&self) -> bool {
        if !self.inner.is_valid() {
            return false;
        }
        match &self.end {
            Bound::Included(end) | Bound::Excluded(end) => {
                self.inner.key().raw_ref() >= end.as_ref()
            }
            Bound::Unbounded => false,
        }
    }

    fn skip_deleted(&mut self) -> Result<()> {
        while self.inner.is_valid() && !self.invalid && self.inner.value().is_empty() {
            if self.reached_end() {
                self.invalid = true;
                return Ok(());
            }
            self.inner.next()?;
        }
        self.check_out_of_bound();
        Ok(())
    }
}

impl StorageIterator for LsmIterator {
    type KeyType<'a> = &'a [u8];

    fn is_valid(&self) -> bool {
        self.inner.is_valid() && !self.invalid
    }

    fn key(&self) -> &[u8] {
        self.inner.key().raw_ref()
    }

    fn value(&self) -> &[u8] {
        self.inner.value()
    }

    fn next(&mut self) -> Result<()> {
        if self.check_out_of_bound() {
            return Ok(());
        }

        if self.reached_end() {
            self.invalid = true;
            return Ok(());
        }

        if self.inner.is_valid() && !self.invalid {
            self.inner.next()?;
        }

        self.skip_deleted()
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
