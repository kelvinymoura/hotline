//! What a revocation ends of a teammate's exchanges, and what an account
//! switch, which is one, keeps of them: the results and answers its turns
//! had already finished.

use super::*;

impl Room {
    pub(in crate::session) fn revoke_exchanges(&self, persona: &str) {
        self.stop_revoked_exchanges(Some(persona));
    }

    /// A switch changes the account a teammate runs on, not what it may do:
    /// what its turns already finished is still its senders' to hear. While
    /// the switch closes the teammate's authority, each such request whose
    /// lease of the teammate's is still current is bound to its sender's in
    /// its place, so neither the switch's revocation nor the worker's sweep
    /// ends it; one a stop or a policy change has already ended stays ended.
    /// Says which it bound, for [`Room::bind_kept_answers`] once the switch
    /// is over.
    pub(in crate::session) fn keep_finished_answers(&self, persona: &str) -> Vec<String> {
        let _guard = lock(&self.exchange_lock);
        let mut leases = lock(&self.exchange_leases);
        let mut kept = Vec::new();
        for request in self.exchange_pairs().iter().flat_map(|pair| &pair.requests) {
            if request.to == persona
                && Self::turn_over(request)
                && Self::exchange_pending(request)
                && let Some((sender, recipient)) = leases.get_mut(&request.id)
                && recipient.is_current()
            {
                *recipient = sender.clone();
                kept.push(request.id.clone());
            }
        }
        kept
    }

    /// Binds what a switch kept to its teammate's authority again, now that
    /// the switch has opened it: a later stop or policy change on the
    /// teammate drops what is still unheard, as it would have without the
    /// switch.
    pub(in crate::session) fn bind_kept_answers(&self, persona: &str, kept: &[String]) {
        let _guard = lock(&self.exchange_lock);
        let mut leases = lock(&self.exchange_leases);
        for request in self
            .exchange_pairs()
            .iter()
            .flat_map(|pair| &pair.requests)
            .filter(|r| kept.contains(&r.id) && Self::exchange_pending(r))
        {
            if let Some((_, recipient)) = leases.get_mut(&request.id) {
                *recipient = self.capability_lease(persona).scoped();
            }
        }
    }

    /// Ends what `persona` holds or was given, and anything whose authority
    /// lapsed. A request `persona` was given and has finished is left to be
    /// heard only while the leases it holds here are current: a stop or a
    /// policy change ends them first, a switch leaves them so (or binds them
    /// to the sender's for as long as it closes the teammate's authority),
    /// and one from before a restart holds none.
    pub(super) fn stop_revoked_exchanges(&self, persona: Option<&str>) {
        let _guard = lock(&self.exchange_lock);
        let (mut stopped, mut ended) = (Vec::new(), Vec::new());
        for mut pair in self.exchange_pairs() {
            let ours = persona.is_some_and(|id| pair.a == id || pair.b == id);
            let mut changed = false;
            for request in &mut pair.requests {
                if ours {
                    changed |= progress::release(request, &mut ended);
                }
                let finished_under_current_authority = persona == Some(request.to.as_str())
                    && Self::turn_over(request)
                    && lock(&self.exchange_leases)
                        .get(&request.id)
                        .is_some_and(|(a, b)| a.is_current() && b.is_current());
                if !Self::exchange_pending(request)
                    || finished_under_current_authority
                    || (!ours && self.exchange_lease_current(&request.id))
                {
                    continue;
                }
                request.owed_to_thread = self.owes_thread(request, persona);
                self.cancel_handoff(request);
                request.phase = Phase::Stopped;
                progress::release(request, &mut ended);
                request.failed = true;
                request.reply = "Collaboration was revoked or a participant stopped. Send again if still needed.".into();
                stopped.push(request.clone());
                changed = true;
            }
            if changed && let Err(error) = self.save_pair(&pair) {
                eprintln!("revoke exchange: {error}");
            }
            self.wake_owed(&pair);
        }
        self.mark_handoffs(&ended);
        self.exchange_notices(&stopped);
    }
}
