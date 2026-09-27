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

use super::StorageIterator;

/// Merges two iterators of different types into one. If the two iterators have the same key, only
/// produce the key once and prefer the entry from A.
/*
A = MergeIterator<MemTableIterator>
    mutable memtable
    + immutable memtables
    newest first

B = MergeIterator<SsTableIterator>
    L0 SSTs
    newest first
*/
pub struct TwoMergeIterator<A: StorageIterator, B: StorageIterator> {
    a: A,
    b: B,
    // Add fields as need
    use_a: bool,
}

impl<
    A: 'static + StorageIterator,
    B: 'static + for<'a> StorageIterator<KeyType<'a> = A::KeyType<'a>>,
> TwoMergeIterator<A, B>
{
    pub fn create(a: A, b: B) -> Result<Self> {
        // 每次初始化或者next之后，都要维护use_a
        let mut use_a = true;
        if !a.is_valid() {
            use_a = false;
        } else if !b.is_valid() {
            use_a = true;
        } else if a.is_valid() && b.is_valid() {
            use_a = a.key() <= b.key();
        }

        Ok(Self { a, b, use_a })
    }
}

impl<
    A: 'static + StorageIterator,
    B: 'static + for<'a> StorageIterator<KeyType<'a> = A::KeyType<'a>>,
> StorageIterator for TwoMergeIterator<A, B>
{
    type KeyType<'a> = A::KeyType<'a>;

    fn key(&self) -> Self::KeyType<'_> {
        if self.use_a {
            self.a.key()
        } else {
            self.b.key()
        }
    }

    fn value(&self) -> &[u8] {
        if self.use_a {
            self.a.value()
        } else {
            self.b.value()
        }
    }

    fn is_valid(&self) -> bool {
        self.a.is_valid() || self.b.is_valid()
    }

    fn next(&mut self) -> Result<()> {
        if self.use_a {
            if self.b.is_valid() && self.a.key() == self.b.key() {
                self.b.next()?;
            }
            self.a.next()?;
        } else {
            // 如果A.key == B.key，选择规则本来就应该选 A
            // 所以这个分支不需要考虑a.next
            self.b.next()?;
        }

        // 更新之后维护use_a
        if !self.a.is_valid() {
            self.use_a = false;
        } else if !self.b.is_valid() {
            self.use_a = true;
        } else if self.a.is_valid() && self.b.is_valid() {
            self.use_a = self.a.key() <= self.b.key();
        }
        Ok(())
    }
}
