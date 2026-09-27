//! Historial de deshacer/rehacer basado en instantáneas.
//!
//! Guarda estados completos (baratos si comparten datos con `Arc`). El llamador decide
//! cuándo un cambio queda confirmado y registra el estado anterior con `record`.

pub struct History<T> {
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
}

impl<T> History<T> {
    pub fn new(limit: usize) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), limit: limit.max(1) }
    }

    /// Registra `previous` (el estado antes del cambio). Un cambio nuevo invalida lo que
    /// se podía rehacer.
    pub fn record(&mut self, previous: T) {
        self.undo.push(previous);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Devuelve el estado anterior; `current` pasa a poder rehacerse.
    pub fn undo(&mut self, current: T) -> Option<T> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// Devuelve el estado siguiente; `current` pasa a poder deshacerse.
    pub fn redo(&mut self, current: T) -> Option<T> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
}

#[cfg(test)]
mod tests {
    use super::History;

    #[test]
    fn undo_and_redo_walk_back_and_forth() {
        let mut history = History::new(10);
        // Estados 0 → 1 → 2 (se registra el anterior a cada cambio).
        history.record(0);
        history.record(1);
        assert_eq!(history.undo(2), Some(1));
        assert_eq!(history.undo(1), Some(0));
        assert_eq!(history.undo(0), None);
        assert_eq!(history.redo(0), Some(1));
        assert_eq!(history.redo(1), Some(2));
        assert_eq!(history.redo(2), None);
    }

    #[test]
    fn new_change_clears_redo() {
        let mut history = History::new(10);
        history.record(0);
        assert_eq!(history.undo(1), Some(0));
        history.record(0); // 0 → 5, en vez de rehacer el 1
        assert_eq!(history.redo(5), None);
        assert_eq!(history.undo(5), Some(0));
    }

    #[test]
    fn oldest_steps_are_dropped_past_the_limit() {
        let mut history = History::new(2);
        for state in 0..5 {
            history.record(state);
        }
        assert_eq!(history.undo(5), Some(4));
        assert_eq!(history.undo(4), Some(3));
        assert_eq!(history.undo(3), None);
    }
}
