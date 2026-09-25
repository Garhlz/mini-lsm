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

use std::cmp::{self};
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;

use anyhow::Result;

use crate::key::KeySlice;

use super::StorageIterator;

struct HeapWrapper<I: StorageIterator>(pub usize, pub Box<I>);

// 允许某个值连自己都“不相等”，如浮点数 NaN
impl<I: StorageIterator> PartialEq for HeapWrapper<I> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

// 不增加方法，要求每个值都等于自身
impl<I: StorageIterator> Eq for HeapWrapper<I> {}

// 可返回None，表示两个值无法比较
impl<I: StorageIterator> PartialOrd for HeapWrapper<I> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// 任意两个值都能得到小于、等于或大于
impl<I: StorageIterator> Ord for HeapWrapper<I> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.1
            .key()
            .cmp(&other.1.key())
            .then(self.0.cmp(&other.0))
            .reverse()
        // BinaryHeap 默认把“最大”的元素放在堆顶。
        // 反转后，较小的 key 会先出来；key 相同时，较小的来源编号会先出来
        // 1.2 规定编号较小的输入更新，所以同一个 key 的新版本优先。
    }
}

/// Merge multiple iterators of the same type. If the same key occurs multiple times in some
/// iterators, prefer the one with smaller index.
pub struct MergeIterator<I: StorageIterator> {
    iters: BinaryHeap<HeapWrapper<I>>,
    current: Option<HeapWrapper<I>>,
}

impl<I: StorageIterator> MergeIterator<I> {
    pub fn create(iters: Vec<Box<I>>) -> Self {
        let heap_items: Vec<_> = iters
            .into_iter()
            .enumerate()
            .filter_map(|(i, iter)| {
                // 不能把已经失效的迭代器放入堆中
                if iter.is_valid() {
                    Some(HeapWrapper(i, iter))
                } else {
                    None
                }
            })
            .collect();

        let mut heap = BinaryHeap::from(heap_items);

        let current = heap.pop();
        Self {
            iters: heap,
            current,
        }
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice<'_> {
        let current = self.current.as_ref();
        match current {
            Some(heap_wrapper) => heap_wrapper.1.key(),
            None => KeySlice::from_slice(&[]),
        }
    }

    fn value(&self) -> &[u8] {
        let current = self.current.as_ref();
        match current {
            Some(heap_wrapper) => heap_wrapper.1.value(),
            None => &[],
        }
    }

    fn is_valid(&self) -> bool {
        let current = self.current.as_ref();
        match current {
            Some(heap_wrapper) => heap_wrapper.1.is_valid(),
            None => false,
        }
    }

    fn next(&mut self) -> Result<()> {
        // current 是刚刚对外展示的版本。暂时取出它，最后再与其他来源
        // 一起竞争下一个最小 key。
        let mut current = self.current.take().unwrap();

        // 堆顶若与 current 同 key，就代表一个应被跳过的旧版本。
        // 只检查堆顶即可：堆顶的 key 一旦不同，其他元素也不会是当前 key。
        while let Some(mut front) = self.iters.peek_mut() {
            if front.1.key() != current.1.key() {
                break;
            }

            // PeekMut 离开作用域时会重新整理堆，因此出错或到达末尾的
            // 子迭代器必须先移出堆，不能让堆排序再读取它的无效 key。
            match front.1.next() {
                Ok(()) if !front.1.is_valid() => {
                    PeekMut::pop(front);
                }
                Ok(()) => {} // guard 释放时，堆按新 key 重新排序。
                Err(err) => {
                    PeekMut::pop(front);
                    return Err(err);
                }
            }
        }

        // current 的这个 key 已输出过；推进它，以便这一路后续的 key
        // 也参与下一轮堆排序。耗尽的迭代器不能再放回堆。
        current.1.next()?;
        if current.1.is_valid() {
            self.iters.push(current);
        }

        // pop 返回当前最小的 key；全部耗尽时返回 None。
        self.current = self.iters.pop();

        Ok(())
    }
}
