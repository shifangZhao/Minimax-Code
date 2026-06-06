// Paced text rendering for smooth typewriter streaming effect
// Uses requestAnimationFrame with adaptive step sizing and word-boundary snapping

import { ref, watch, onUnmounted } from 'vue'

const TEXT_SNAP = /[\s.,!?;:)\]]/

export function usePacedText(
  fullText: () => string,
  isDone: () => boolean,
) {
  const displayedText = ref('')
  let rafId: number | null = null
  let active = true

  function cancel() {
    if (rafId !== null) {
      cancelAnimationFrame(rafId)
      rafId = null
    }
  }

  function step() {
    if (!active || isDone()) {
      cancel()
      return
    }

    const full = fullText()
    const current = displayedText.value
    const remaining = full.length - current.length

    if (remaining <= 0) {
      rafId = null
      return
    }

    // Catch-up: if lagging, jump to near-current instantly
    if (remaining > 200) {
      displayedText.value = full.slice(0, full.length - 80)
      rafId = requestAnimationFrame(step)
      return
    }

    // Adaptive step: faster for longer texts (max 80 = 4800 chars/sec at 60fps)
    let size: number
    if (remaining < 12) size = 6
    else if (remaining < 48) size = 12
    else if (remaining < 96) size = 24
    else size = Math.min(Math.ceil(remaining / 4), 80)

    let end = current.length + size
    if (end < full.length) {
      // Look ahead up to 6 chars for a natural break point
      const slice = full.slice(end, Math.min(end + 6, full.length))
      const m = slice.match(TEXT_SNAP)
      if (m && m.index !== undefined) {
        end += m.index + 1
      }
    }

    displayedText.value = full.slice(0, Math.min(end, full.length))
    rafId = requestAnimationFrame(step)
  }

  // Snap to final text when done; reset when new stream starts
  watch(isDone, (done, wasDone) => {
    if (done) {
      cancel()
      displayedText.value = fullText()
    } else if (wasDone) {
      cancel()
      displayedText.value = ''
    }
  })

  // Start the stepper when new text arrives
  watch(
    () => fullText(),
    (text) => {
      if (text && !isDone() && rafId === null && active) {
        rafId = requestAnimationFrame(step)
      }
    },
  )

  onUnmounted(() => {
    active = false
    cancel()
  })

  return { displayedText }
}
