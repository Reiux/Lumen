import { ref } from 'vue'

export const launchReplayTrigger = ref(0)

export function replayLaunch(): void {
  launchReplayTrigger.value++
}
