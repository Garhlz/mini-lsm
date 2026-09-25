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

mod builder;
mod iterator;
use crate::key::{KeySlice, KeyVec};

pub use builder::BlockBuilder;
use bytes::{Buf, BufMut, Bytes};
pub use iterator::BlockIterator;

/// A block is the smallest unit of read and caching in LSM tree. It is a collection of sorted key-value pairs.
pub struct Block {
    pub(crate) data: Vec<u8>,
    pub(crate) offsets: Vec<u16>,
}

impl Block {
    /// Encode the internal data to the data layout illustrated in the course
    /// Note: You may want to recheck if any of the expected field is missing from your output
    pub fn encode(&self) -> Bytes {
        let mut buf: Vec<u8> = vec![];
        buf.put_slice(&self.data);
        for &offset in self.offsets.iter() {
            buf.put_u16(offset);
        }
        let offset_len = u16::try_from(self.offsets.len()).expect("条目数超过 u16");
        buf.put_u16(offset_len);
        Bytes::from(buf)
    }

    /// Decode from the data layout, transform the input `data` to a single `Block`
    pub fn decode(data: &[u8]) -> Self {
        let len = data.len();

        let mut count_buf = &data[len - 2..];
        let count = count_buf.get_u16() as usize;

        let offset_start = len - 2 - count * 2;
        let block_data = data[..offset_start].to_vec();

        let mut offset_buf = &data[offset_start..len - 2];
        let mut offsets = Vec::with_capacity(count);

        for _ in 0..count {
            offsets.push(offset_buf.get_u16());
        }

        Block {
            data: block_data,
            offsets,
        }
    }

    // 获取index位置上的offset对应的键值对的key
    pub fn get_key_at(&self, index: usize) -> KeyVec {
        let mut offset = self.offsets[index] as usize;

        let mut key_len_buf = &self.data[offset..offset + 2];
        let key_len = key_len_buf.get_u16() as usize;

        offset += 2;
        let key_buf = self.data[offset..offset + key_len].to_vec();

        KeyVec::from_vec(key_buf)
    }

    // 获取index位置上的offset对应的键值对的value range
    pub fn get_value_range_at(&self, index: usize) -> (usize, usize) {
        let mut offset = self.offsets[index] as usize;
        let mut key_len_buf = &self.data[offset..offset + 2];
        let key_len = key_len_buf.get_u16() as usize;

        offset += 2 + key_len;
        let mut value_len_buf = &self.data[offset..offset + 2];
        let value_len = value_len_buf.get_u16() as usize;
        offset += 2;
        (offset, offset + value_len)
    }

    // 获取index位置上的offset对应的键值对的key和value range，减少一点内存访问
    pub fn get_kv_at(&self, index: usize) -> (KeyVec, (usize, usize)) {
        let mut offset = self.offsets[index] as usize;

        let mut key_len_buf = &self.data[offset..offset + 2];
        let key_len = key_len_buf.get_u16() as usize;

        offset += 2;
        let key_buf = self.data[offset..offset + key_len].to_vec();
        let key = KeyVec::from_vec(key_buf);

        offset += key_len;
        let mut value_len_buf = &self.data[offset..offset + 2];
        let value_len = value_len_buf.get_u16() as usize;
        offset += 2;

        (key, (offset, offset + value_len))
    }

    // 获取第一个大于等于输入key的kv的offset index
    pub fn binary_search(&self, key: KeySlice) -> usize {
        let offset_len = self.offsets.len();
        let mut l = 0;
        let mut r = offset_len;
        while l < r {
            let mid = l + (r - l) / 2;
            if self.get_key_at(mid).as_key_slice() >= key {
                r = mid;
            } else {
                l = mid + 1;
            }
        }
        l
    }
}
