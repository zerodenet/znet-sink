package org.zerodenet.znetsink.nativeio

import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.atomic.AtomicInteger
import kotlin.concurrent.thread

class NativePluginRunGateTest {
    private val id = "0123456789abcdef0123456789abcdef"
    @Test fun systemStopSealsLateDeliveryAndClaimsAndroidCompletionExactlyOnce() {
        val gate = NativePluginRunGate(id)
        assertTrue(gate.stop())
        assertTrue(gate.stopped())
        assertFalse(gate.complete())
        assertFalse(gate.stop())
        assertTrue(gate.claimCallback())
        assertFalse(gate.claimCallback())
    }
    @Test fun completedReceiptCannotBeReplacedByLateSystemStop() {
        val gate = NativePluginRunGate(id)
        assertTrue(gate.complete())
        assertFalse(gate.stop())
        assertFalse(gate.stopped())
        assertFalse(gate.complete())
        assertTrue(gate.claimCallback())
        assertFalse(gate.claimCallback())
    }
    @Test fun stopAndCompletionRaceHasOneWinner() {
        repeat(100) {
            val gate = NativePluginRunGate(id)
            val ready = CountDownLatch(1)
            val winners = AtomicInteger()
            val callbackWinners = AtomicInteger()
            val threads = listOf(
                thread { ready.await(); if (gate.stop()) winners.incrementAndGet(); if (gate.claimCallback()) callbackWinners.incrementAndGet() },
                thread { ready.await(); if (gate.complete()) winners.incrementAndGet(); if (gate.claimCallback()) callbackWinners.incrementAndGet() }
            )
            ready.countDown(); threads.forEach { it.join() }
            assertEquals(1, winners.get())
            assertEquals(1, callbackWinners.get())
        }
    }
    @Test fun onlyUnfinishedPreviousProcessRunsBecomeUnknown() {
        assertTrue(NativePluginRunGate.needsRecovery("running", "old", "new"))
        assertFalse(NativePluginRunGate.needsRecovery("running", "same", "same"))
        for (state in listOf("completed", "failed", "deferred", "stopped", "result-unknown"))
            assertFalse(NativePluginRunGate.needsRecovery(state, "old", "new"))
    }
    @Test fun runIdentitiesCannotBePathsOrGuestSuppliedLabels() {
        assertTrue(NativePluginRunGate.validId(id))
        for (invalid in listOf("", "../task", "0123456789abcdef", id.uppercase(), "$id\n")) {
            assertFalse(NativePluginRunGate.validId(invalid))
            try { NativePluginRunGate(invalid); fail("invalid run ID admitted") } catch (_: IllegalArgumentException) { }
        }
    }
}
