<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useClientsList } from "../composables/useClientsList";
import { useI18n } from "../composables/useI18n";

const { t, number, d } = useI18n();
const { data, loading, error, lastUpdated, load } = useClientsList();

onMounted(load);

const totalPeers = computed(() =>
  data.value ? data.value.clients.reduce((s, c) => s + c.peers, 0) : 0,
);

const rows = computed(() => data.value?.clients ?? []);

const statusText = computed(() => {
  if (loading.value) return t("top100_loading");
  if (error.value) return t("top100_error");
  if (lastUpdated.value)
    return `${t("last_update")} ${d(lastUpdated.value, "time")}`;
  return "";
});

function share(peers: number): string {
  if (!totalPeers.value) return "—";
  return `${((peers / totalPeers.value) * 100).toFixed(1)}%`;
}
</script>

<template>
  <section class="flex justify-between items-start gap-5 mb-6 max-[900px]:flex-col max-[900px]:items-stretch">
    <div>
      <h1 class="m-0 mb-1.5 text-[28px] leading-tight max-[560px]:text-[24px] font-bold">{{ t('clients_title') }}</h1>
      <p class="m-0 text-muted text-sm leading-relaxed">{{ t('clients_subtitle') }}</p>
    </div>
  </section>

  <section class="bg-panel border border-line p-4 mb-5">
    <div class="flex items-center justify-between gap-4 mb-3 max-[900px]:flex-col max-[900px]:items-stretch">
      <div class="flex items-center justify-end gap-3 w-full">
        <span class="text-muted text-xs whitespace-nowrap">{{ statusText }}</span>
        <UButton
          :disabled="loading"
          variant="none"
          :class="[
            'border border-line bg-panel text-ink px-4 text-[13px] cursor-pointer min-h-8 rounded hover:bg-hover-soft',
            loading ? 'opacity-50 cursor-not-allowed' : '',
          ]"
          @click="load"
        >
          {{ t('refresh') }}
        </UButton>
      </div>
    </div>

    <div class="overflow-x-auto">
      <table class="w-full border-collapse text-[13px]">
        <thead>
          <tr>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap w-12 text-center">#</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap">{{ t('clients_col_name') }}</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">{{ t('sort_peers') }}</th>
            <th class="text-left p-2.5 bg-soft text-muted font-semibold text-xs uppercase border-b-2 border-line whitespace-nowrap text-right">{{ t('clients_col_share') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="loading">
            <td colspan="4" class="p-8 text-center text-muted">{{ t('top100_loading') }}</td>
          </tr>
          <tr v-else-if="error">
            <td colspan="4" class="p-8 text-center text-bad">{{ t('top100_error') }}</td>
          </tr>
          <tr v-else-if="!rows.length">
            <td colspan="4" class="p-8 text-center text-muted">{{ t('top100_empty') }}</td>
          </tr>
          <template v-else>
            <tr
              v-for="(row, i) in rows"
              :key="row.name"
              class="hover:bg-row-hover"
            >
              <td class="p-2 px-3 border-b border-td-border text-center text-muted font-semibold w-12">{{ i + 1 }}</td>
              <td class="p-2 px-3 border-b border-td-border font-medium">{{ row.name }}</td>
              <td class="p-2 px-3 border-b border-td-border text-right whitespace-nowrap tabular-nums">{{ number(row.peers) }}</td>
              <td class="p-2 px-3 border-b border-td-border text-right whitespace-nowrap tabular-nums text-muted">{{ share(row.peers) }}</td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </section>
</template>
