//! Deciding which node a new hub goes on.
//!
//! This used to be `map.iter().next()` — the first entry of a HashMap
//! iteration, which is to say an arbitrary connected agent, with no notion of
//! how many hubs it already had. A farm operator could not say "this server
//! holds five hubs, that one holds three", because there was nowhere to say it
//! and nothing that would have read it.
//!
//! Two kinds of node: registered server agents, and the farm's own process
//! (which hosts hubs itself when no agent is connected). Both take a cap;
//! `None` means unlimited.
//!
//! Placement is **refused, never overflowed**. Silently putting a sixth hub on
//! a node capped at five would make the cap decorative, and the operator set it
//! for a reason we cannot see from here — RAM, disk, a licence, a noisy
//! neighbour.

/// A node that can host hubs, with its capacity as the operator set it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// `None` for the farm's own process; the server id for an agent.
    pub server_id: Option<String>,
    pub in_use: i64,
    /// `None` means unlimited.
    pub capacity: Option<i64>,
}

impl Node {
    pub fn has_room(&self) -> bool {
        match self.capacity {
            None => true,
            Some(cap) => self.in_use < cap,
        }
    }

    /// Free slots, or `i64::MAX` when uncapped — used only for ordering.
    fn free(&self) -> i64 {
        match self.capacity {
            None => i64::MAX,
            Some(cap) => cap - self.in_use,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlacementError {
    /// The requested server is not a known, connected agent.
    UnknownServer,
    /// The requested server is full.
    ServerFull,
    /// Nothing has room.
    NoCapacity,
}

impl PlacementError {
    pub fn code(&self) -> &'static str {
        match self {
            PlacementError::UnknownServer => "unknown_server",
            PlacementError::ServerFull => "server_full",
            PlacementError::NoCapacity => "no_capacity",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            PlacementError::UnknownServer => {
                "that server is not registered, or its agent is not connected"
            }
            PlacementError::ServerFull => "that server is already at its hub limit",
            PlacementError::NoCapacity => {
                "every server is at its hub limit — raise one, or register another"
            }
        }
    }
}

/// Choose a node for a new hub.
///
/// An explicit `requested` server is honoured or refused, never quietly
/// redirected: an operator who named a server wants *that* one, and landing
/// elsewhere would be discovered much later and much more confusingly than a
/// 409.
///
/// Otherwise the emptiest node wins — hubs spread out instead of piling onto
/// whichever one happened to be first. Ties break toward agents over the farm's
/// own process, so the farm stays free for the work only it can do.
pub fn choose<'a>(nodes: &'a [Node], requested: Option<&str>) -> Result<&'a Node, PlacementError> {
    if let Some(id) = requested {
        let node = nodes
            .iter()
            .find(|n| n.server_id.as_deref() == Some(id))
            .ok_or(PlacementError::UnknownServer)?;
        return if node.has_room() {
            Ok(node)
        } else {
            Err(PlacementError::ServerFull)
        };
    }

    nodes
        .iter()
        .filter(|n| n.has_room())
        .max_by_key(|n| (n.free(), n.server_id.is_some()))
        .ok_or(PlacementError::NoCapacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, in_use: i64, capacity: Option<i64>) -> Node {
        Node {
            server_id: Some(id.to_string()),
            in_use,
            capacity,
        }
    }

    fn local(in_use: i64, capacity: Option<i64>) -> Node {
        Node {
            server_id: None,
            in_use,
            capacity,
        }
    }

    /// The operator's example: one server holds five, the other three.
    #[test]
    fn caps_are_respected_per_server() {
        let nodes = vec![agent("s1", 5, Some(5)), agent("s2", 1, Some(3))];
        assert_eq!(
            choose(&nodes, None).unwrap().server_id.as_deref(),
            Some("s2"),
            "the full server must not take a sixth hub"
        );
    }

    #[test]
    fn the_emptiest_node_wins_so_hubs_spread_out() {
        let nodes = vec![agent("s1", 4, Some(10)), agent("s2", 1, Some(10))];
        assert_eq!(
            choose(&nodes, None).unwrap().server_id.as_deref(),
            Some("s2")
        );
    }

    #[test]
    fn an_uncapped_node_is_always_available() {
        let nodes = vec![agent("s1", 999, None)];
        assert!(choose(&nodes, None).is_ok());
    }

    /// Refused, not redirected: an operator who named a server wants that one.
    #[test]
    fn an_explicit_choice_is_honoured_or_refused() {
        let nodes = vec![agent("s1", 5, Some(5)), agent("s2", 0, Some(3))];
        assert_eq!(choose(&nodes, Some("s1")), Err(PlacementError::ServerFull));
        assert_eq!(
            choose(&nodes, Some("s2")).unwrap().server_id.as_deref(),
            Some("s2")
        );
        assert_eq!(
            choose(&nodes, Some("nope")),
            Err(PlacementError::UnknownServer)
        );
    }

    #[test]
    fn everything_full_is_an_error_not_an_overflow() {
        let nodes = vec![agent("s1", 3, Some(3)), local(2, Some(2))];
        assert_eq!(choose(&nodes, None), Err(PlacementError::NoCapacity));
        assert_eq!(choose(&[], None), Err(PlacementError::NoCapacity));
    }

    /// With equal room, an agent takes the hub — the farm process has work
    /// only it can do (routing, auth, supervision) and should stay free for it.
    #[test]
    fn a_tie_goes_to_an_agent_rather_than_the_farm_itself() {
        let nodes = vec![local(0, Some(5)), agent("s1", 0, Some(5))];
        assert_eq!(
            choose(&nodes, None).unwrap().server_id.as_deref(),
            Some("s1")
        );
    }

    /// The farm's own process is a real placement target, not a fallback that
    /// bypasses the accounting — capping it has to actually cap it.
    #[test]
    fn the_farm_itself_is_capped_like_any_other_node() {
        let nodes = vec![local(5, Some(5))];
        assert_eq!(choose(&nodes, None), Err(PlacementError::NoCapacity));
        assert!(choose(&[local(4, Some(5))], None).is_ok());
    }
}
