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
    key::{KeyBytes, KeySlice, KeyVec},
    lsm_storage::BlockCache,
    table::{FileObject, bloom::Bloom},
};
use anyhow::Result;
use bytes::{Buf, BufMut};

/// Builds an SSTable from key-value pairs.
pub struct SsTableBuilder {
    builder: BlockBuilder,
    first_key: Vec<u8>,
    last_key: Vec<u8>,
    data: Vec<u8>,
    pub(crate) meta: Vec<BlockMeta>,
    block_size: usize,
    hashed_keys: Vec<u32>,
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
            hashed_keys: Vec::new(),
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

            // 维护hashed keys
            let h = farmhash::fingerprint32(key.raw_ref());
            self.hashed_keys.push(h);
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

        // 在末尾维护bloom filter
        let bloom = encode_bloom(&self.hashed_keys, &mut self.data);

        let file = FileObject::create(path.as_ref(), self.data)?;

        Ok(SsTable {
            file,
            block_meta: self.meta,
            block_meta_offset,
            id,
            block_cache,
            first_key: KeyBytes::from_bytes(self.first_key.into()),
            last_key: KeyBytes::from_bytes(self.last_key.into()),
            bloom: Some(bloom),
            max_ts: 0,
        })
    }

    #[cfg(test)]
    pub(crate) fn build_for_test(self, path: impl AsRef<Path>) -> Result<SsTable> {
        self.build(0, None, path)
    }
}

fn encode_bloom(hashed_keys: &[u32], buf: &mut Vec<u8>) -> Bloom {
    // 要求误判率为0.01
    let bits_per_key = Bloom::bloom_bits_per_key(hashed_keys.len(), 0.01);
    let bloom = Bloom::build_from_key_hashes(hashed_keys, bits_per_key);
    let bloom_offset = buf.len();
    bloom.encode(buf);
    buf.put_u32(bloom_offset as u32);
    bloom
}

pub fn encode_shared_prefix(key: KeySlice, first_key: KeySlice) -> Vec<u8> {
    let prefix_len = key
        .raw_ref()
        .iter()
        .zip(first_key.raw_ref().iter())
        .take_while(|(x, y)| x == y)
        .count();

    let rest_len = key.len() - prefix_len;

    let mut buf: Vec<u8> = Vec::new();
    buf.put_u16(prefix_len as u16);
    buf.put_u16(rest_len as u16);
    buf.put_slice(&key.raw_ref()[prefix_len..]);
    buf
}

pub fn decode_shared_prefix(mut data: impl Buf, first_key: KeySlice) -> KeyVec {
    let prefix_len_buf = data.get_u16() as usize;
    let rest_len = data.get_u16() as usize;

    let mut rest_key = vec![0u8; rest_len];
    data.copy_to_slice(&mut rest_key);

    let mut prefix_key = first_key.raw_ref()[..prefix_len_buf].to_vec();
    prefix_key.extend(rest_key);
    KeyVec::from_vec(prefix_key)
}
