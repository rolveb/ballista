// Copyright 2021 Andy Grove
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! ShuffleReaderExec reads partitions that have already been materialized by an executor.

use std::any::Any;
use std::sync::Arc;

use crate::client::BallistaClient;
use crate::memory_stream::MemoryStream;
use crate::scheduler::planner::PartitionLocation;

use arrow::datatypes::SchemaRef;
use async_trait::async_trait;
use datafusion::error::{DataFusionError, Result};
use datafusion::physical_plan::{ExecutionPlan, Partitioning, SendableRecordBatchStream};
use log::info;

/// ShuffleReaderExec reads partitions that have already been materialized by an executor.
#[derive(Debug, Clone)]
pub struct ShuffleReaderExec {
    // The query stage that is responsible for producing the shuffle partitions that
    // this operator will read
    pub(crate) partition_location: Vec<PartitionLocation>,
    pub(crate) schema: SchemaRef,
}

impl ShuffleReaderExec {
    /// Create a new ShuffleReaderExec
    pub fn try_new(partition_meta: Vec<PartitionLocation>, schema: SchemaRef) -> Result<Self> {
        Ok(Self {
            partition_location: partition_meta,
            schema,
        })
    }
}

#[async_trait]
impl ExecutionPlan for ShuffleReaderExec {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }

    fn output_partitioning(&self) -> Partitioning {
        Partitioning::UnknownPartitioning(self.partition_location.len())
    }

    fn children(&self) -> Vec<Arc<dyn ExecutionPlan>> {
        vec![]
    }

    fn with_new_children(
        &self,
        _children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Err(DataFusionError::Plan(
            "Ballista ShuffleReaderExec does not support with_new_children()".to_owned(),
        ))
    }

    async fn execute(&self, partition: usize) -> Result<SendableRecordBatchStream> {
        info!("ShuffleReaderExec::execute({})", partition);
        let partition_location = &self.partition_location[partition];

        let mut client = BallistaClient::try_new(
            &partition_location.executor_meta.host,
            partition_location.executor_meta.port as usize,
        )
        .await
        .map_err(|e| DataFusionError::Execution(format!("Ballista Error: {:?}", e)))?;

        let batches = client
            .fetch_partition(
                &partition_location.partition_id.job_uuid,
                partition_location.partition_id.stage_id,
                partition,
            )
            .await
            .map_err(|e| DataFusionError::Execution(format!("Ballista Error: {:?}", e)))?;

        Ok(Box::pin(MemoryStream::try_new(
            batches,
            self.schema(),
            None,
        )?))
    }
}
