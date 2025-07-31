//! Token trading graph for atomic arbitrage operations.
//!
//! This module provides a specialized graph data structure for modeling token trading networks
//! where nodes represent tokens/assets and edges represent liquidity pools or trading pairs.
//! The graph is optimized for arbitrage path discovery and execution.

pub mod types;
pub mod core;

// Re-export all public types for convenience
pub use types::{TokenId, PoolId, PoolInfo, TokenNode, LiquidityPool};
pub use core::TradingGraph;

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tycho_common::Bytes;

    #[test]
    fn test_add_token() {
        let mut graph = TradingGraph::new();
        let token_address = Bytes::from_str("0x1111").unwrap();

        let token_id = graph.add_token(token_address.clone());

        assert!(token_id.is_ok());
        assert_eq!(graph.token_count(), 1);
        
        // Test idempotency
        let token_id2 = graph.add_token(token_address);
        assert!(token_id2.is_ok());
        assert_eq!(token_id.unwrap(), token_id2.unwrap());
        assert_eq!(graph.token_count(), 1);
    }

    #[test]
    fn test_remove_token() {
        let mut graph = TradingGraph::new();

        let token0 = Bytes::from_str("0x0000").unwrap();
        let token1 = Bytes::from_str("0x0001").unwrap();
        let token2 = Bytes::from_str("0x0002").unwrap();
        let token3 = Bytes::from_str("0x0003").unwrap();

        let idx0 = graph.add_token(token0).unwrap();
        let idx1 = graph.add_token(token1).unwrap();
        let idx2 = graph.add_token(token2).unwrap();
        let idx3 = graph.add_token(token3).unwrap();

        let pool0 = Bytes::from_str("0x1000").unwrap();
        let pool1 = Bytes::from_str("0x1001").unwrap();
        let pool2 = Bytes::from_str("0x1002").unwrap();
        let pool3 = Bytes::from_str("0x1003").unwrap();

        let _ = graph.add_pool(pool0, [idx0, idx1]);
        let _ = graph.add_pool(pool1, [idx1, idx2]);
        let _ = graph.add_pool(pool2, [idx0, idx2]);
        let _ = graph.add_pool(pool3, [idx0, idx1]);

        assert!(graph.remove_token(idx3).is_ok());
        assert_eq!(graph.token_count(), 3);
    }

    #[test]
    fn test_add_pool() {
        let mut graph = TradingGraph::new();

        let token1 = Bytes::from_str("0x0001").unwrap();
        let token2 = Bytes::from_str("0x0002").unwrap();
        let token3 = Bytes::from_str("0x0003").unwrap();
        let token4 = Bytes::from_str("0x0004").unwrap();

        let idx1 = graph.add_token(token1).unwrap();
        let idx2 = graph.add_token(token2).unwrap();
        let idx3 = graph.add_token(token3).unwrap();
        let _idx4 = graph.add_token(token4).unwrap();

        let pool1 = Bytes::from_str("0x1001").unwrap();
        let pool2 = Bytes::from_str("0x1002").unwrap();
        let pool3 = Bytes::from_str("0x1003").unwrap();
        let pool4 = Bytes::from_str("0x1004").unwrap();

        assert!(graph.add_pool(pool1, [idx1, idx2]).is_ok());
        assert!(graph.add_pool(pool2, [idx2, idx3]).is_ok());
        assert!(graph.add_pool(pool3, [idx1, idx3]).is_ok());
        assert!(graph.add_pool(pool4, [idx1, idx2]).is_ok());
    }

    #[test]
    fn test_remove_pool() {
        let mut graph = TradingGraph::new();

        let token1 = Bytes::from_str("0x0001").unwrap();
        let token2 = Bytes::from_str("0x0002").unwrap();
        let token3 = Bytes::from_str("0x0003").unwrap();
        let token4 = Bytes::from_str("0x0004").unwrap();

        let idx1 = graph.add_token(token1).unwrap();
        let idx2 = graph.add_token(token2).unwrap();
        let idx3 = graph.add_token(token3).unwrap();
        let _idx4 = graph.add_token(token4).unwrap();

        let pool1 = Bytes::from_str("0x1001").unwrap();
        let pool2 = Bytes::from_str("0x1002").unwrap();
        let pool3 = Bytes::from_str("0x1003").unwrap();
        let pool4 = Bytes::from_str("0x1004").unwrap();

        let _ = graph.add_pool(pool1.clone(), [idx1, idx2]);
        let _ = graph.add_pool(pool2, [idx2, idx3]);
        let _ = graph.add_pool(pool3, [idx1, idx3]);
        let _ = graph.add_pool(pool4.clone(), [idx1, idx2]);

        let pools_12 = graph.pools_between_tokens([idx1, idx2]);
        assert!(pools_12.is_ok());
        assert_eq!(pools_12.unwrap().len(), 2);

        let _ = graph.remove_pool_by_address(&pool1);
        let pools_12 = graph.pools_between_tokens([idx1, idx2]);
        assert!(pools_12.is_ok());
        assert_eq!(pools_12.unwrap().len(), 1);
    }

    #[test]
    fn test_protocol_component_integration() {
        let mut graph = TradingGraph::new();
        
        // Create a mock protocol component
        let token1_addr = Bytes::from_str("0x0001").unwrap();
        let token2_addr = Bytes::from_str("0x0002").unwrap();
        let pool_addr = Bytes::from_str("0x1001").unwrap();
        
        let protocol_component = tycho_simulation::protocol::models::ProtocolComponent {
            id: pool_addr.clone(),
            address: pool_addr.clone(),
            protocol_system: "test".to_string(),
            protocol_type_name: "test_pool".to_string(),
            chain: tycho_common::models::Chain::Ethereum,
            tokens: vec![
                tycho_simulation::models::Token {
                    address: token1_addr.clone(),
                    symbol: "TOKEN1".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token2_addr.clone(),
                    symbol: "TOKEN2".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
            ],
            contract_ids: vec![pool_addr.clone()],
            static_attributes: std::collections::HashMap::new(),
            created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            creation_tx: tycho_common::Bytes::default(),
        };

        let pool_infos = graph.add_protocol_component(pool_addr.clone(), protocol_component);
        assert!(pool_infos.is_ok());
        
        let infos = pool_infos.unwrap();
        assert_eq!(infos.len(), 1); // 2 tokens = 1 pair
        assert_eq!(infos[0].token_ids.len(), 2);
        assert_eq!(infos[0].pool_ids.len(), 2);
        
        // Verify tokens were added
        assert_eq!(graph.token_count(), 2);
        assert_eq!(graph.pool_count(), 1);
        
        // Test removal - all pools with the same address will be removed
        assert!(graph.remove_protocol_component(&pool_addr).is_ok());
        assert_eq!(graph.pool_count(), 0);
    }

    #[test]
    fn test_protocol_component_three_tokens() {
        let mut graph = TradingGraph::new();
        
        // Create a mock protocol component with 3 tokens
        let token1_addr = Bytes::from_str("0x0001").unwrap();
        let token2_addr = Bytes::from_str("0x0002").unwrap();
        let token3_addr = Bytes::from_str("0x0003").unwrap();
        let pool_addr = Bytes::from_str("0x1001").unwrap();
        
        let protocol_component = tycho_simulation::protocol::models::ProtocolComponent {
            id: pool_addr.clone(),
            address: pool_addr.clone(),
            protocol_system: "test".to_string(),
            protocol_type_name: "test_pool".to_string(),
            chain: tycho_common::models::Chain::Ethereum,
            tokens: vec![
                tycho_simulation::models::Token {
                    address: token1_addr.clone(),
                    symbol: "TOKEN1".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token2_addr.clone(),
                    symbol: "TOKEN2".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token3_addr.clone(),
                    symbol: "TOKEN3".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
            ],
            contract_ids: vec![pool_addr.clone()],
            static_attributes: std::collections::HashMap::new(),
            created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            creation_tx: tycho_common::Bytes::default(),
        };

        let pool_infos = graph.add_protocol_component(pool_addr.clone(), protocol_component);
        assert!(pool_infos.is_ok());
        
        let infos = pool_infos.unwrap();
        assert_eq!(infos.len(), 3); // 3 tokens = 3 pairs: (1,2), (1,3), (2,3)
        
        // Verify all pairs were created
        let token1_id = graph.find_token_id(&token1_addr).unwrap();
        let token2_id = graph.find_token_id(&token2_addr).unwrap();
        let token3_id = graph.find_token_id(&token3_addr).unwrap();
        
        // Check that all tokens are neighbors of each other
        let token1_neighbors = graph.token_neighbors(token1_id).unwrap();
        assert!(token1_neighbors.contains(&token2_id));
        assert!(token1_neighbors.contains(&token3_id));
        assert_eq!(token1_neighbors.len(), 2);
        
        let token2_neighbors = graph.token_neighbors(token2_id).unwrap();
        assert!(token2_neighbors.contains(&token1_id));
        assert!(token2_neighbors.contains(&token3_id));
        assert_eq!(token2_neighbors.len(), 2);
        
        let token3_neighbors = graph.token_neighbors(token3_id).unwrap();
        assert!(token3_neighbors.contains(&token1_id));
        assert!(token3_neighbors.contains(&token2_id));
        assert_eq!(token3_neighbors.len(), 2);
        
        // Verify tokens and pools were added correctly
        assert_eq!(graph.token_count(), 3);
        assert_eq!(graph.pool_count(), 1); // 1 unique pool address shared by all pairs
        
        // Test removal
        assert!(graph.remove_protocol_component(&pool_addr).is_ok());
        assert_eq!(graph.pool_count(), 0);
    }

    #[test]
    fn test_protocol_component_four_tokens() {
        let mut graph = TradingGraph::new();
        
        // Create a mock protocol component with 4 tokens
        let token1_addr = Bytes::from_str("0x0001").unwrap();
        let token2_addr = Bytes::from_str("0x0002").unwrap();
        let token3_addr = Bytes::from_str("0x0003").unwrap();
        let token4_addr = Bytes::from_str("0x0004").unwrap();
        let pool_addr = Bytes::from_str("0x1001").unwrap();
        
        let protocol_component = tycho_simulation::protocol::models::ProtocolComponent {
            id: pool_addr.clone(),
            address: pool_addr.clone(),
            protocol_system: "test".to_string(),
            protocol_type_name: "test_pool".to_string(),
            chain: tycho_common::models::Chain::Ethereum,
            tokens: vec![
                tycho_simulation::models::Token {
                    address: token1_addr.clone(),
                    symbol: "TOKEN1".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token2_addr.clone(),
                    symbol: "TOKEN2".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token3_addr.clone(),
                    symbol: "TOKEN3".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: token4_addr.clone(),
                    symbol: "TOKEN4".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
            ],
            contract_ids: vec![pool_addr.clone()],
            static_attributes: std::collections::HashMap::new(),
            created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            creation_tx: tycho_common::Bytes::default(),
        };

        let pool_infos = graph.add_protocol_component(pool_addr.clone(), protocol_component);
        assert!(pool_infos.is_ok());
        
        let infos = pool_infos.unwrap();
        assert_eq!(infos.len(), 6); // 4 tokens = 6 pairs: (1,2), (1,3), (1,4), (2,3), (2,4), (3,4)
        
        // Verify all pairs were created
        let token1_id = graph.find_token_id(&token1_addr).unwrap();
        let token2_id = graph.find_token_id(&token2_addr).unwrap();
        let token3_id = graph.find_token_id(&token3_addr).unwrap();
        let token4_id = graph.find_token_id(&token4_addr).unwrap();
        
        // Check that all tokens are neighbors of each other
        for &token_id in &[token1_id, token2_id, token3_id, token4_id] {
            let neighbors = graph.token_neighbors(token_id).unwrap();
            assert_eq!(neighbors.len(), 3); // Each token should have 3 neighbors
            
            // Verify this token is connected to all other tokens
            for &other_token_id in &[token1_id, token2_id, token3_id, token4_id] {
                if token_id != other_token_id {
                    assert!(neighbors.contains(&other_token_id));
                }
            }
        }
        
        // Verify tokens and pools were added correctly
        assert_eq!(graph.token_count(), 4);
        assert_eq!(graph.pool_count(), 1); // 1 unique pool address shared by all pairs
        
        // Test removal
        assert!(graph.remove_protocol_component(&pool_addr).is_ok());
        assert_eq!(graph.pool_count(), 0);
    }

    #[test]
    fn test_protocol_component_invalid_token_counts() {
        let mut graph = TradingGraph::new();
        let pool_addr = Bytes::from_str("0x1001").unwrap();
        
        // Test with 1 token (should fail)
        let protocol_component_1_token = tycho_simulation::protocol::models::ProtocolComponent {
            id: pool_addr.clone(),
            address: pool_addr.clone(),
            protocol_system: "test".to_string(),
            protocol_type_name: "test_pool".to_string(),
            chain: tycho_common::models::Chain::Ethereum,
            tokens: vec![
                tycho_simulation::models::Token {
                    address: Bytes::from_str("0x0001").unwrap(),
                    symbol: "TOKEN1".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
            ],
            contract_ids: vec![pool_addr.clone()],
            static_attributes: std::collections::HashMap::new(),
            created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            creation_tx: tycho_common::Bytes::default(),
        };
        
        let result = graph.add_protocol_component(pool_addr.clone(), protocol_component_1_token);
        assert!(result.is_err());
        
        // Test with 5 tokens (should fail)
        let protocol_component_5_tokens = tycho_simulation::protocol::models::ProtocolComponent {
            id: pool_addr.clone(),
            address: pool_addr.clone(),
            protocol_system: "test".to_string(),
            protocol_type_name: "test_pool".to_string(),
            chain: tycho_common::models::Chain::Ethereum,
            tokens: vec![
                tycho_simulation::models::Token {
                    address: Bytes::from_str("0x0001").unwrap(),
                    symbol: "TOKEN1".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: Bytes::from_str("0x0002").unwrap(),
                    symbol: "TOKEN2".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: Bytes::from_str("0x0003").unwrap(),
                    symbol: "TOKEN3".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: Bytes::from_str("0x0004").unwrap(),
                    symbol: "TOKEN4".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
                tycho_simulation::models::Token {
                    address: Bytes::from_str("0x0005").unwrap(),
                    symbol: "TOKEN5".to_string(),
                    decimals: 18,
                    gas: num_bigint::BigUint::from(0u32),
                },
            ],
            contract_ids: vec![pool_addr.clone()],
            static_attributes: std::collections::HashMap::new(),
            created_at: chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            creation_tx: tycho_common::Bytes::default(),
        };
        
        let result = graph.add_protocol_component(pool_addr, protocol_component_5_tokens);
        assert!(result.is_err());
    }

    #[test]
    fn test_domain_specific_methods() {
        let mut graph = TradingGraph::new();
        
        let usdc = Bytes::from_str("0xa0b86a33e6441e6c7d3e4c2a4c0b3c4d5e6f7890").unwrap();
        let weth = Bytes::from_str("0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2").unwrap();
        let dai = Bytes::from_str("0x6b175474e89094c44da98b954eedeac495271d0f").unwrap();
        
        let usdc_id = graph.add_token(usdc.clone()).unwrap();
        let weth_id = graph.add_token(weth.clone()).unwrap();
        let _dai_id = graph.add_token(dai.clone()).unwrap();
        
        // Test token lookup
        assert_eq!(graph.find_token_id(&usdc).unwrap(), usdc_id);
        assert_eq!(graph.find_token_id(&weth).unwrap(), weth_id);
        
        // Test token retrieval
        let usdc_node = graph.get_token(usdc_id).unwrap();
        assert_eq!(usdc_node.address(), &usdc);
        assert_eq!(usdc_node.neighbor_count(), 0);
        
        // Add a pool and test neighbors
        let pool_addr = Bytes::from_str("0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8").unwrap();
        let _ = graph.add_pool(pool_addr.clone(), [usdc_id, weth_id]).unwrap();
        
        let usdc_neighbors = graph.token_neighbors(usdc_id).unwrap();
        assert!(usdc_neighbors.contains(&weth_id));
        assert_eq!(usdc_neighbors.len(), 1);
        
        let weth_neighbors = graph.token_neighbors(weth_id).unwrap();
        assert!(weth_neighbors.contains(&usdc_id));
        assert_eq!(weth_neighbors.len(), 1);
        
        // Test pool lookup
        let pools = graph.pools_between_tokens([usdc_id, weth_id]).unwrap();
        assert_eq!(pools.len(), 1);
        
        let pool = graph.get_pool(pools[0]).unwrap();
        assert_eq!(pool.address(), &pool_addr);
        assert_eq!(pool.token_in_id(), usdc_id);
        assert_eq!(pool.token_out_id(), weth_id);
    }
}
