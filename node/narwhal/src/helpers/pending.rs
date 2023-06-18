// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkOS library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:
// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::helpers::{Entry, EntryID};
use snarkvm::console::prelude::*;

use parking_lot::RwLock;
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug)]
pub struct Pending<N: Network> {
    /// The set of pending `entry IDs`.
    entries: Arc<RwLock<HashSet<EntryID<N>>>>,
}

impl<N: Network> Default for Pending<N> {
    /// Initializes a new instance of the pending queue.
    fn default() -> Self {
        Self::new()
    }
}

impl<N: Network> Pending<N> {
    /// Initializes a new instance of the pending queue.
    pub fn new() -> Self {
        Self { entries: Default::default() }
    }

    /// Inserts the specified `entry ID` to the pending queue.
    pub fn insert(&self, entry_id: impl Into<EntryID<N>>) {
        self.entries.write().insert(entry_id.into());
    }

    /// Removes the specified `entry ID` from the pending queue.
    pub fn remove(&self, entry_id: impl Into<EntryID<N>>) {
        self.entries.write().remove(&entry_id.into());
    }
}

impl<N: Network> Deref for Pending<N> {
    type Target = RwLock<HashSet<EntryID<N>>>;

    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}