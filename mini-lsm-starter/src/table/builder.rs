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

use std::path::Path;
use std::sync::Arc;

use super::{BlockMeta, SsTable};
use crate::{
    block::BlockBuilder,
    key::{KeyBytes, KeySlice},
    lsm_storage::BlockCache,
    table::FileObject,
};
use anyhow::Result;
use bytes::BufMut;

/// Builds an SSTable from key-value pairs.
pub struct SsTableBuilder {
    builder: BlockBuilder,
    first_key: Vec<u8>,
    last_key: Vec<u8>,
    data: Vec<u8>,
    pub(crate) meta: Vec<BlockMeta>,
    block_size: usize,
}

impl SsTableBuilder {
    /// Create a builder based on target block size.
    pub fn new(block_size: usize) -> Self {
        Self {
            builder: BlockBuilder::new(block_size),
            first_key: Vec::new(),
            last_key: Vec::new(),
            data: Vec::new(),
            meta: Vec::new(),
            block_size,
        }
    }

    /// Adds a key-value pair to SSTable.
    ///
    /// Note: You should split a new block when the current block is full.(`std::mem::replace` may
    /// be helpful here)
    pub fn add(&mut self, key: KeySlice, value: &[u8]) {
        if self.builder.add(key, value) {
            // 成功加入当前block builder对应的block
            let new_key = key.to_key_vec().raw_ref().to_vec();
            assert!(self.last_key < new_key);
            // 统一在这里维护first key,last key
            self.last_key = new_key.clone();

            if self.first_key.is_empty() {
                self.first_key = new_key;
            }
            return;
        }

        // 当前的builder满了，需要它生成的block插入sst中
        let old_builder = std::mem::replace(&mut self.builder, BlockBuilder::new(self.block_size));

        let block = old_builder.build();

        let block_meta = BlockMeta {
            offset: self.data.len(),
            first_key: block.get_key_at(0).into_key_bytes(),
            last_key: block.get_key_at(block.offsets.len() - 1).into_key_bytes(),
        };
        self.meta.push(block_meta);

        let data = &block.encode();
        self.data.put_slice(data);

        // 重试
        self.add(key, value);
    }

    /// Get the estimated size of the SSTable.
    ///
    /// Since the data blocks contain much more data than meta blocks, just return the size of data
    /// blocks here.
    pub fn estimated_size(&self) -> usize {
        self.data.len()
    }

    /// Builds the SSTable and writes it to the given path. Use the `FileObject` structure to manipulate the disk objects.
    pub fn build(
        #[allow(unused_mut)] mut self,
        id: usize,
        block_cache: Option<Arc<BlockCache>>,
        path: impl AsRef<Path>,
    ) -> Result<SsTable> {
        if !self.builder.is_empty() {
            // 编码的时候，还要把当前没有写完的block builder写入data
            let old_builder = self.builder;

            let block = old_builder.build();

            let block_meta = BlockMeta {
                offset: self.data.len(),
                first_key: block.get_key_at(0).into_key_bytes(),
                last_key: block.get_key_at(block.offsets.len() - 1).into_key_bytes(),
            };
            self.meta.push(block_meta);

            let data = &block.encode();
            self.data.put_slice(data);
        }

        // 然后正常编码
        let block_meta_offset = BlockMeta::encode_block_meta(&self.meta, &mut self.data);

        let file = FileObject::create(path.as_ref(), self.data)?;

        Ok(SsTable {
            file,
            block_meta: self.meta,
            block_meta_offset,
            id,
            block_cache,
            first_key: KeyBytes::from_bytes(self.first_key.into()),
            last_key: KeyBytes::from_bytes(self.last_key.into()),
            bloom: None,
            max_ts: 0,
        })
    }

    #[cfg(test)]
    pub(crate) fn build_for_test(self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.build(0, None, path)
    }
}
