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

use bytes::BufMut;

use crate::key::{KeySlice, KeyVec};

use super::Block;

/// Builds a block.
pub struct BlockBuilder {
    /// Offsets of each key-value entries.
    offsets: Vec<u16>,
    /// All serialized key-value pairs in the block.
    data: Vec<u8>,
    /// The expected block size.
    block_size: usize,
    /// The first key in the block
    first_key: KeyVec,
}

impl BlockBuilder {
    /// Creates a new block builder.
    pub fn new(block_size: usize) -> Self {
        Self {
            offsets: vec![],
            data: vec![],
            block_size,
            first_key: KeyVec::new(),
        }
    }
    fn get_size(&self) -> usize {
        // 每个offset每条是2字节，最后还有u16表示的entry数量
        self.data.len() + self.offsets.len() * 2 + 2
    }
    /// Adds a key-value pair to the block. Returns false when the block is full.
    /// You may find the `bytes::BufMut` trait useful for manipulating binary data.
    #[must_use]
    pub fn add(&mut self, key: KeySlice, value: &[u8]) -> bool {
        // 不应该静默截断
        let key_len = u16::try_from(key.len()).expect("key 长度超过 u16");
        let value_len = u16::try_from(value.len()).expect("value 长度超过 u16");
        // 判断是否超过block size
        let new_size = 2 + key.len() + 2 + value.len() + 2;
        // block 可以因为单个超大 KV 而超过 target size
        if !self.is_empty() && self.get_size() + new_size > self.block_size {
            return false;
        }
        let offset = u16::try_from(self.data.len()).expect("offset超过 u16");
        self.offsets.push(offset);

        self.data.put_u16(key_len);
        self.data.put_slice(key.raw_ref());
        self.data.put_u16(value_len);
        self.data.put_slice(value);
        if self.first_key.is_empty() {
            self.first_key.append(key.raw_ref());
        }
        true
    }

    /// Check if there is no key-value pair in the block.
    pub fn is_empty(&self) -> bool {
        self.first_key.is_empty()
    }

    /// Finalize the block.
    pub fn build(self) -> Block {
        Block {
            data: self.data,
            offsets: self.offsets,
        }
    }
}
