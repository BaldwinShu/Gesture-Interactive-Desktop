<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { openPath } from '@tauri-apps/plugin-opener';
import { initHandDetection } from './composables/useHandDetection';

const closeApp = async () => {
  try { await invoke('show_desktop_icons'); } catch {}
  try { await invoke('exit_app'); } catch (e) { console.error('退出失败:', e); }
};

// ---- 顶栏信息 ----
const username = ref('');
const currentTime = ref('');
let timeInterval: ReturnType<typeof setInterval>;

function updateTime() {
  const d = new Date();
  const pad = (n: number) => n.toString().padStart(2, '0');
  currentTime.value = `${d.getFullYear()}/${pad(d.getMonth()+1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

async function loadUsername() {
  try { username.value = await invoke('get_username'); }
  catch { username.value = 'User'; }
}

// ---- 设置面板 ----
const settingsOpen = ref(false);
const gearRef = ref<HTMLElement>();
const animOrigin = ref({ x: 0, y: 0 });

function openSettings(e: MouseEvent) {
  const btn = (e.currentTarget as HTMLElement);
  const r = btn.getBoundingClientRect();
  animOrigin.value = { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  settingsOpen.value = true;
}
function closeSettings() { settingsOpen.value = false; }

// ---- 桌面图标 ----
interface DesktopItem {
  name: string; path: string; iconUrl: string; category: string;
}
const desktopItems = ref<DesktopItem[]>([]);
const isLoading = ref(true);

const CATEGORY_ICONS: Record<string, { icon: string; accent: string }> = {
  '高效工作':       { icon: '💼', accent: '#4fc3f7' },
  '创造力':         { icon: '🎨', accent: '#ff8a65' },
  '开发人员工具':   { icon: '⚙️', accent: '#81c784' },
  '实用程序与工具': { icon: '🔧', accent: '#aed581' },
  '娱乐':           { icon: '🎮', accent: '#ef5350' },
  '社交':           { icon: '💬', accent: '#7c4dff' },
  '辅助功能':       { icon: '♿', accent: '#ffd54f' },
  '其他':           { icon: '📁', accent: '#90a4ae' },
  '文件':           { icon: '📄', accent: '#78909c' },
};
const CATEGORY_ORDER = [
  '高效工作', '创造力', '开发人员工具', '实用程序与工具',
  '娱乐', '社交', '辅助功能', '其他', '文件',
];

const loadDesktop = async () => {
  isLoading.value = true;
  try {
    const raw: Array<{ name: string; path: string; category: string }> =
      await invoke('get_categorized_desktop_items');
    const items = await Promise.all(raw.map(async (item) => {
      let iconUrl = '';
      try { iconUrl = await invoke('get_file_icon_base64', { path: item.path, size: 64 }); }
      catch { /* no icon */ }
      return { name: item.name, path: item.path, iconUrl, category: item.category };
    }));
    desktopItems.value = items;
    await nextTick(); updateCarouselSize();
  } catch (e) { console.error('加载桌面失败:', e); }
  finally { isLoading.value = false; }
};

const openFile = async (path: string) => {
  try { await openPath(path); }
  catch (e) { console.error(`无法打开 ${path}:`, e); }
};

// ---- 轮播 ----
const expandedKey = ref<string | null>(null);
const carouselRef = ref<HTMLDivElement>();
const viewportWidth = ref(0);
const currentCard = ref(0);

function scrollLeft() {
  const n = effectiveGroupedItems.value.length;
  currentCard.value = (currentCard.value - 1 + n) % n;
}
function scrollRight() {
  const n = effectiveGroupedItems.value.length;
  currentCard.value = (currentCard.value + 1) % n;
}

// 重排卡片 + 平滑滑动 + 3D 效果
const cardTransforms = computed(() => {
  const items = effectiveGroupedItems.value;
  const n = items.length;
  if (!n) return [];
  const half = Math.floor(n / 2);
  const result = [];
  for (let i = 0; i < n; i++) {
    const srcIdx = ((currentCard.value - half + i) % n + n) % n;
    const diff = Math.abs(i - half);
    // translateX: 大幅度偏移实现平滑滑动效果
    const direction = Math.sign(i - half);
    const slideShift = direction * diff * 80;
    result.push({
      item: items[srcIdx],
      key: items[srcIdx].key,
      scale: Math.max(0.5, 1 - diff * 0.18),
      opacity: Math.max(0.3, 1 - diff * 0.22),
      translateX: slideShift,
      z: 100 - diff * 15,
    });
  }
  return result;
});

// ---- 手势惯性引擎 ----
function expandGroup(key: string) { expandedKey.value = key; expandedCardIdx.value = 0; }
function collapseGroup() { expandedKey.value = null; }
const expandedGroup = computed(() => effectiveGroupedItems.value.find(g => g.key === expandedKey.value));

// 展开层 3D 滚轮
const expandedCardIdx = ref(0);

const expandedCardTransforms = computed(() => {
  const items = expandedGroup.value?.items ?? [];
  const n = items.length;
  if (!n) return [];
  const half = Math.floor(n / 2);
  const result = [];
  for (let i = 0; i < n; i++) {
    const srcIdx = ((expandedCardIdx.value - half + i) % n + n) % n;
    const diff = Math.abs(i - half);
    result.push({
      item: items[srcIdx],
      scale: Math.max(0.55, 1 - diff * 0.18),
      opacity: Math.max(0.3, 1 - diff * 0.22),
      translateX: Math.sign(i - half) * diff * 80,
      z: 100 - diff * 15,
    });
  }
  return result;
});

function expandScroll(dir: number) {
  const n = expandedGroup.value?.items.length ?? 0;
  if (!n) return;
  expandedCardIdx.value = ((expandedCardIdx.value + dir) % n + n) % n;
}

function updateCarouselSize() {
  if (carouselRef.value) viewportWidth.value = carouselRef.value.clientWidth;
}

// ---- 设置面板 - 自定义分类 ----
interface CategoryDef {
  name: string;
  displayName: string;
  isBuiltin: boolean;
}

const customCategories = ref<CategoryDef[]>([]);
const editingCategory = ref<string | null>(null);
const editName = ref('');

function addCategory() {
  const name = `自定义${customCategories.value.length + 1}`;
  customCategories.value.push({ name, displayName: name, isBuiltin: false });
}
function removeCategory(name: string) {
  customCategories.value = customCategories.value.filter(c => c.name !== name);
}
function startEdit(name: string) {
  editingCategory.value = name;
  editName.value = customCategories.value.find(c => c.name === name)?.displayName || name;
}
function saveEdit() {
  if (editingCategory.value && editName.value.trim()) {
    const c = customCategories.value.find(c => c.name === editingCategory.value);
    if (c) c.displayName = editName.value.trim();
  }
  editingCategory.value = null;
}

// ---- 自定义拖拽（鼠标事件，兼容 WebView2）----
const fileAssign = ref<Record<string, string>>({});
const dragging = ref<{ path: string; name: string; iconUrl: string } | null>(null);
const dragPos = ref({ x: 0, y: 0 });
const dragOverCat = ref<string | null>(null);

function startDrag(e: MouseEvent, file: DesktopItem) {
  e.preventDefault();
  dragging.value = { path: file.path, name: file.name, iconUrl: file.iconUrl };
  dragPos.value = { x: e.clientX, y: e.clientY };
  document.addEventListener('mousemove', onDragMove);
  document.addEventListener('mouseup', onDragEnd);
}

function onDragMove(e: MouseEvent) {
  dragPos.value = { x: e.clientX, y: e.clientY };
  const el = document.elementFromPoint(e.clientX, e.clientY);
  const zone = el?.closest('[data-category]') as HTMLElement | null;
  dragOverCat.value = zone?.getAttribute('data-category') || null;
}

async function onDragEnd(_e: MouseEvent) {
  document.removeEventListener('mousemove', onDragMove);
  document.removeEventListener('mouseup', onDragEnd);

  if (dragging.value && dragOverCat.value) {
    if (fileAssign.value[dragging.value.path] !== dragOverCat.value) {
      fileAssign.value = { ...fileAssign.value, [dragging.value.path]: dragOverCat.value };
      try {
        await invoke('save_category_config', {
          config: { categories: {}, fileCategories: fileAssign.value }
        });
      } catch { /* */ }
    }
  }
  dragging.value = null;
  dragOverCat.value = null;
}

// 合并服务器分类与用户自定义覆盖
function getEffectiveCategory(item: { path: string; category: string }): string {
  return fileAssign.value[item.path] || item.category;
}

// 按有效分类重新分组
const effectiveGroupedItems = computed(() => {
  const map: Record<string, DesktopItem[]> = {};
  for (const item of desktopItems.value) {
    const cat = getEffectiveCategory(item);
    if (!map[cat]) map[cat] = [];
    map[cat].push(item);
  }
  return CATEGORY_ORDER
    .filter(cat => map[cat]?.length > 0)
    .map(cat => ({
      key: cat, label: cat,
      ...(CATEGORY_ICONS[cat] || { icon: '📁', accent: '#90a4ae' }),
      items: map[cat].sort((a, b) => a.name.localeCompare(b.name)),
    }));
});

// ---- Dock ----
interface SystemWindow { hwnd: number; title: string; processName: string; processPath: string | null; }
const systemWindows = ref<SystemWindow[]>([]);
const iconCache: Record<string, string> = {};

const loadIconForProcess = async (p: string | null, n: string) => {
  if (iconCache[n] !== undefined) return;
  try { iconCache[n] = await invoke('get_file_icon_base64', { path: p || n, size: 48 }); }
  catch { iconCache[n] = ''; }
};
const fetchWindows = async () => {
  try {
    const w: SystemWindow[] = await invoke('get_system_windows');
    for (const x of w) { if (iconCache[x.processName] === undefined) loadIconForProcess(x.processPath, x.processName); }
    systemWindows.value = w;
  } catch { /* */ }
};
const switchToWindow = async (h: number) => { try { await invoke('switch_to_window', { hwnd: h }); } catch { /* */ } };

let windowsInterval: ReturnType<typeof setInterval>;
let unlisten: () => void;

// ---- 挂载 ----
onMounted(async () => {
  try { await invoke('hide_desktop_icons'); } catch {}
  try { await invoke('move_self_to_bottom'); } catch {}
  loadUsername();
  updateTime(); timeInterval = setInterval(updateTime, 1000);
  await loadDesktop();
  await invoke('start_watcher');
  unlisten = await listen('files-changed', async () => { await loadDesktop(); await nextTick(); updateCarouselSize(); });
  await fetchWindows();
  windowsInterval = setInterval(fetchWindows, 3000);

  const ro = new ResizeObserver(updateCarouselSize);
  if (carouselRef.value) ro.observe(carouselRef.value);
  window.addEventListener('resize', updateCarouselSize);
});

onUnmounted(() => {
  try { invoke('show_desktop_icons'); } catch {}
  if (unlisten) unlisten();
  clearInterval(timeInterval);
  clearInterval(windowsInterval);
  window.removeEventListener('resize', updateCarouselSize);
  if (animationId) cancelAnimationFrame(animationId);
  if (stopCamera) stopCamera();
});

// ---- 手势（检测 + 滚轮控制）----
const videoRef = ref<HTMLVideoElement>();
const canvasRef = ref<HTMLCanvasElement>();
let animationId: number;
let stopCamera: (() => void) | undefined;
const connections = [[0,1],[1,2],[2,3],[3,4],[0,5],[5,6],[6,7],[7,8],[0,9],[9,10],[10,11],[11,12],[0,13],[13,14],[14,15],[15,16],[0,17],[17,18],[18,19],[19,20]];

const gestureHint = ref('');
const gestureStartX = ref(0);
let gestureSwipeActive = false;

onMounted(async () => {
  if (!videoRef.value || !canvasRef.value) return;
  const { detectHands, analyzeGesture, stopCamera: stop } = await initHandDetection(videoRef.value);
  stopCamera = stop;
  const ctx = canvasRef.value.getContext('2d'); if (!ctx) return;

  let pushActive = false;
  let fistCount = 0;
  let mouseActive = false;
  let _mouseMissCount = 0;
  let _mouseEverActivated = false;
  let _mouseSmoothX = 0;
  let _mouseSmoothY = 0;
  let _mouseScreenW = 1920;
  let _mouseScreenH = 1080;
  let mouseWasDown = false;
  let _mouseClickFlash = 0;
  let _thumbBentFrames = 0;  // 拇指弯曲稳定帧计数器
  let lastClickTime = 0;       // 上次点击触发时间戳
  const CLICK_COOLDOWN = 500;  // 点击冷却时间（毫秒）

  const draw = async () => {
    if (videoRef.value && canvasRef.value) {
      const w = videoRef.value.videoWidth, h = videoRef.value.videoHeight;
      if (w && h) { canvasRef.value.width = w; canvasRef.value.height = h; }
    }
    ctx.clearRect(0, 0, canvasRef.value!.width, canvasRef.value!.height);
    const hands = await detectHands();
    const g = analyzeGesture(hands);

    for (const hand of hands) {
      for (const p of hand.keypoints) { ctx.beginPath(); ctx.arc(p.x, p.y, 5, 0, Math.PI * 2); ctx.fillStyle = '#00ff00'; ctx.fill(); }
      for (const [s, e] of connections) {
        const p1 = hand.keypoints[s], p2 = hand.keypoints[e];
        if (p1 && p2) { ctx.beginPath(); ctx.moveTo(p1.x, p1.y); ctx.lineTo(p2.x, p2.y); ctx.strokeStyle = '#ffffff'; ctx.lineWidth = 2; ctx.stroke(); }
      }
    }

    // 距离判定：手部跨度归一化值×1000，越大=越近，≈100 = 15cm
    const dist = g.handDist;
    const isNear = dist > 160; // 160 ≈ 30cm

    // 画距离到摄像头画面
    ctx.fillStyle = isNear ? 'rgba(255,200,50,0.9)' : 'rgba(255,255,255,0.5)';
    ctx.font = 'bold 18px monospace';
    ctx.fillText(`📏${dist.toFixed(0)} ${isNear ? '📌近区' : '🌐远区'}`, 10, 28);

    // === 远区 > 15cm → 仅开掌滑动 ===
    if (!isNear) {
      if (g.count >= 1 && g.anyOpenPalm) {
        const px = g.avgPalmX;
        if (!gestureSwipeActive) { gestureSwipeActive = true; gestureStartX.value = px; gestureHint.value = '👋 滑动';
          // 开掌滑动时关闭鼠标
          if (mouseActive) { mouseActive = false; if (mouseWasDown) { mouseWasDown = false; invoke('mouse_up', { button: 'left' }).catch(() => {}); } }
        }
        else {
          const dx = px - gestureStartX.value;
          if (dx > 22) { (expandedKey.value ? expandScroll(-1) : scrollLeft()); gestureStartX.value = px; }
          else if (dx < -22) { (expandedKey.value ? expandScroll(1) : scrollRight()); gestureStartX.value = px; }
        }
      } else if (gestureSwipeActive) { gestureSwipeActive = false; }

      // 记录 point 姿势
      if (g.anyPointing) pushActive = true;
      else if (!g.anyPointing) pushActive = false;
      fistCount = 0;
    }

    // （鼠标操作功能已取消）

    // === 近区 < 15cm → 打开 / 关闭 ===
    if (isNear) {
      gestureSwipeActive = false;

      // 打开：远区保持 point → 进入近区时 point 还在（0.5秒冷却）
      if (pushActive && g.anyPointing) {
        pushActive = false;
        const now = Date.now();
        if (now - lastClickTime >= CLICK_COOLDOWN) {
          lastClickTime = now;
          if (!expandedKey.value) {
            // 第一层 → 展开分类
            const ci = cardTransforms.value.findIndex((c: any) => c.z >= 90);
            const t = cardTransforms.value[ci >= 0 ? ci : Math.floor(cardTransforms.value.length/2)];
            if (t) { expandGroup(t.key); gestureHint.value = '📂'; setTimeout(() => { if (gestureHint.value === '📂') gestureHint.value = ''; }, 500); }
          } else if (expandedCardTransforms.value.length > 0) {
            // 第二层 → 打开居中文件/快捷方式
            const centerIdx = Math.floor(expandedCardTransforms.value.length / 2);
            const card = expandedCardTransforms.value[centerIdx];
            if (card?.item?.path) { openFile(card.item.path); gestureHint.value = '🚀'; setTimeout(() => gestureHint.value = '', 300); }
          }
        }
      }

      // 关闭：展开层握拳（0.5秒冷却）
      if (expandedKey.value && g.anyFist) {
        fistCount++;
        if (fistCount > 8) {
          const now = Date.now();
          if (now - lastClickTime >= CLICK_COOLDOWN) {
            lastClickTime = now;
            collapseGroup(); gestureHint.value = '🔙'; setTimeout(() => gestureHint.value = '', 400);
          }
          fistCount = 0;
        }
      } else { fistCount = Math.max(0, fistCount - 1); }

      if (!g.anyPointing && !g.anyFist) pushActive = false;
    }

    animationId = requestAnimationFrame(draw);
  };
  draw();
});
</script>

<template>
  <div class="app-container">

    <!-- 顶栏 -->
    <div class="top-bar">
      <div class="top-bar-left">
        <span class="top-time">{{ currentTime }}</span>
        <span class="top-user">👤 {{ username }}</span>
      </div>
      <div class="top-bar-right">
        <button ref="gearRef" class="gear-btn" @click="openSettings">⚙️</button>
      </div>
    </div>

    <!-- 设置面板 -->
    <Transition name="settings-popup">
      <div v-if="settingsOpen" class="settings-overlay" @click.self="closeSettings">
        <div
          class="settings-panel"
          :style="{
            '--origin-x': animOrigin.x + 'px',
            '--origin-y': animOrigin.y + 'px',
          }"
        >
          <div class="settings-header">
            <span>⚙️ 设置 — 自定义分类</span>
            <button class="settings-close" @click="closeSettings">✕</button>
          </div>

          <div class="settings-body">
            <!-- 左侧：应用图标列表 -->
            <div class="assign-layout">
              <div class="assign-files">
                <div class="assign-section-title">📦 桌面文件（拖入右侧分栏）</div>
                <div class="file-scroll">
                  <div class="file-list-inner">
                    <div v-for="f in desktopItems" :key="f.path" class="file-card"
                      @mousedown.prevent="startDrag($event, f)"
                    >
                      <div class="file-card-icon">
                        <img v-if="f.iconUrl" :src="f.iconUrl" />
                      </div>
                      <div class="file-card-name">{{ f.name }}</div>
                    </div>
                  </div>
                </div>
              </div>

              <!-- 右侧：分栏放置区 -->
              <div class="assign-categories">
                <div class="assign-section-title">📂 分栏（拖入文件以归类）</div>
                <div class="drop-zones">
                  <div v-for="cat in CATEGORY_ORDER" :key="cat"
                    class="drop-zone"
                    :class="{ 'drag-over': dragOverCat === cat }"
                    :style="{ '--accent': (CATEGORY_ICONS[cat]?.accent || '#666') }"
                    :data-category="cat"
                  >
                    <span class="drop-zone-icon">{{ CATEGORY_ICONS[cat]?.icon || '📁' }}</span>
                    <span class="drop-zone-label">{{ cat }}</span>
                    <span class="drop-zone-count">{{ effectiveGroupedItems.find(g => g.key === cat)?.items.length || 0 }}项</span>
                  </div>

                  <!-- 自定义分栏 -->
                  <div v-for="cat in customCategories" :key="cat.name"
                    class="drop-zone custom"
                    :class="{ 'drag-over': dragOverCat === cat.displayName }"
                    style="--accent:#888"
                    :data-category="cat.displayName"
                  >
                    <span class="drop-zone-icon">📂</span>
                    <input v-if="editingCategory === cat.name" v-model="editName"
                      class="drop-zone-input" @blur="saveEdit" @keyup.enter="saveEdit" @click.stop autofocus />
                    <span v-else class="drop-zone-label custom-name" @dblclick="startEdit(cat.name)">{{ cat.displayName }}</span>
                    <button class="drop-zone-del" @click.stop="removeCategory(cat.name)" title="删除">✕</button>
                  </div>

                  <button class="add-zone-btn" @click="addCategory">+ 添加分栏</button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </Transition>

    <!-- 手势滚轮状态提示 -->
    <div v-if="gestureHint" class="gesture-hint">{{ gestureHint }}</div>

    <!-- 拖拽幽灵元素 -->
    <div v-if="dragging" class="drag-ghost" :style="{ left: dragPos.x + 'px', top: dragPos.y + 'px' }">
      <div class="drag-ghost-icon">
        <img v-if="dragging.iconUrl" :src="dragging.iconUrl" />
      </div>
      <span class="drag-ghost-name">{{ dragging.name }}</span>
    </div>

    <!-- 主区域 -->
    <div class="desktop-area" :class="{ 'has-dock': systemWindows.length > 0 }">
      <div v-if="isLoading" class="loading-indicator">正在加载桌面...</div>

      <Transition name="view-switch" mode="out-in">
        <div v-if="!expandedKey" key="carousel" class="carousel-wrapper">
          <button v-if="cardTransforms.length > 0" class="carousel-arrow left" @click="scrollLeft">‹</button>

          <div ref="carouselRef" class="carousel-viewport">
            <div class="carousel-track">
              <div v-for="card in cardTransforms" :key="card.key" class="group-card"
                :style="{
                  '--accent': card.item.accent,
                  transform: `translateX(${card.translateX}px) scale(${card.scale})`,
                  opacity: card.opacity,
                  zIndex: card.z,
                }" @click="expandGroup(card.key)">

                <div class="card-label" :style="{ background: card.item.accent + '22' }">
                  {{ card.item.label }}
                </div>

                <div class="card-preview">
                  <div v-for="item in card.item.items.slice(0, 4)" :key="item.path" class="preview-icon" :title="item.name">
                    <img v-if="item.iconUrl" :src="item.iconUrl" />
                  </div>
                  <div v-if="card.item.items.length > 4" class="preview-more">+{{ card.item.items.length - 4 }}</div>
                </div>
              </div>
            </div>
          </div>

          <button v-if="cardTransforms.length > 0" class="carousel-arrow right" @click="scrollRight">›</button>
        </div>

        <div v-else key="expanded" class="expanded-view">
          <div v-if="expandedGroup" class="expanded-panel" :style="{ '--accent': expandedGroup.accent }">
            <div class="expanded-header">
              <button class="back-btn" @click="collapseGroup">← 返回</button>
              <span class="expanded-icon">{{ expandedGroup.icon }}</span>
              <span class="expanded-title">{{ expandedGroup.label }}</span>
              <span class="expanded-count">{{ expandedGroup.items.length }} 项</span>
            </div>
            <div class="expanded-carousel">
              <button class="carousel-arrow left-sm" @click="expandScroll(-1)">‹</button>
              <div class="expanded-track">
                <div v-for="card in expandedCardTransforms" :key="card.item.path" class="expanded-item-card"
                  :style="{
                    transform: `translateX(${card.translateX}px) scale(${card.scale})`,
                    opacity: card.opacity,
                    zIndex: card.z,
                  }" @dblclick="openFile(card.item.path)">
                  <div class="expanded-item-icon">
                    <img v-if="card.item.iconUrl" :src="card.item.iconUrl" />
                  </div>
                  <div class="expanded-item-name">{{ card.item.name }}</div>
                </div>
              </div>
              <button class="carousel-arrow right-sm" @click="expandScroll(1)">›</button>
            </div>
          </div>
        </div>
      </Transition>
    </div>

    <!-- Dock -->
    <div class="dock" v-if="systemWindows.length > 0">
      <div class="dock-inner">
        <div v-for="win in systemWindows" :key="win.hwnd" class="dock-item" @click="switchToWindow(win.hwnd)" :title="win.title">
          <div class="dock-icon"><img v-if="iconCache[win.processName]?.startsWith('data:')" :src="iconCache[win.processName]" /></div>
          <div class="dock-label">{{ win.processName.replace('.exe', '') }}</div>
          <div class="dock-tooltip">{{ win.title }}</div>
        </div>
      </div>
    </div>

    <button class="exit-btn" @click="closeApp">✕ 退出</button>
    <div class="camera-floating">
      <video ref="videoRef" autoplay muted playsinline></video>
      <canvas ref="canvasRef"></canvas>
    </div>
  </div>
</template>

<style scoped>
* { margin:0; padding:0; box-sizing:border-box; }
.app-container {
  width:100vw; height:100vh; overflow:hidden;
  background:transparent; position:relative;
}

/* ---- 顶栏 ---- */
.top-bar {
  position:fixed; top:0; left:0; right:0; height:36px;
  z-index:2000;
  display:flex; align-items:center; justify-content:space-between;
  padding:0 14px;
  background:rgba(0,0,0,0.7);
  backdrop-filter:blur(8px); -webkit-backdrop-filter:blur(8px);
  border-bottom:1px solid rgba(255,255,255,0.06);
  user-select:none;
}
.top-bar-left { display:flex; align-items:center; gap:14px; }
.top-time { color:rgba(255,255,255,0.75); font-size:13px; letter-spacing:0.3px; }
.top-user { color:rgba(255,255,255,0.5); font-size:12px; }
.gear-btn {
  width:30px; height:30px; border-radius:8px;
  border:1px solid rgba(255,255,255,0.15);
  background:#000;
  color:rgba(255,255,255,0.5);
  font-size:16px; line-height:1;
  cursor:pointer; display:flex; align-items:center; justify-content:center;
  transition:all 0.2s;
}
.gear-btn:hover { border-color:rgba(255,255,255,0.3); color:white; background:#111; }

/* ---- 设置面板 ---- */
.settings-overlay {
  position:fixed; inset:0; z-index:1900;
  display:flex; justify-content:flex-end; align-items:center;
  padding-top:36px;
}
.settings-panel {
  width:440px; max-height:60vh; overflow:hidden;
  margin:0 8px;
  background:rgba(15,15,20,0.85);
  backdrop-filter:blur(24px); -webkit-backdrop-filter:blur(24px);
  border:1px solid rgba(255,255,255,0.08);
  border-radius:14px;
  box-shadow:0 8px 48px rgba(0,0,0,0.5);
  transform-origin:var(--origin-x) var(--origin-y);
  display:flex; flex-direction:column;
}
.settings-header {
  display:flex; align-items:center; justify-content:space-between;
  padding:16px 18px; border-bottom:1px solid rgba(255,255,255,0.06);
  font-size:14px; color:rgba(255,255,255,0.85); font-weight:600;
}
.settings-close {
  width:28px; height:28px; border-radius:6px;
  border:1px solid rgba(255,255,255,0.1); background:transparent;
  color:rgba(255,255,255,0.4); font-size:13px;
  cursor:pointer; transition:all 0.15s;
}
.settings-close:hover { background:rgba(232,17,35,0.3); color:white; }
.settings-body { padding:14px 16px; flex:1; overflow:hidden; }

/* 左右分栏布局 */
.assign-layout {
  display:flex; gap:12px; flex:1; min-height:0;
}
.assign-section-title {
  font-size:11px; color:rgba(255,255,255,0.35);
  margin-bottom:8px; flex-shrink:0;
}

/* 左侧和右侧：统一卡片样式 */
.assign-files, .assign-categories {
  display:flex; flex-direction:column;
  background:rgba(255,255,255,0.03);
  border:1px solid rgba(255,255,255,0.06);
  border-radius:10px;
  padding:10px;
}
.assign-files { flex:1; min-width:0; min-height:0; }
.assign-categories { width:150px; flex-shrink:0; min-height:0; }

/* 滚动区域 — min-height:0 让 overflow 生效 */
.file-scroll {
  flex:1; min-height:0; overflow-y:auto; overflow-x:hidden;
  margin-top:4px;
}
.drop-zones {
  display:flex; flex-direction:column; gap:4px;
  flex:1; min-height:0; overflow-y:auto;
  margin-top:4px;
}
.file-list-inner {
  display:flex; flex-direction:column; gap:4px;
}
.file-card {
  display:flex; align-items:center; gap:8px;
  padding:6px 10px; border-radius:8px;
  background:rgba(255,255,255,0.03);
  border:1px solid rgba(255,255,255,0.05);
  cursor:pointer; transition:all 0.15s;
  user-select:none; min-height:40px;
}
.file-card:hover { background:rgba(255,255,255,0.08); border-color:rgba(255,255,255,0.15); }
.file-card:active { transform:scale(0.97); opacity:0.7; }
.file-card-icon { width:36px; height:36px; flex-shrink:0; display:flex; align-items:center; justify-content:center; }
.file-card-icon img { width:100%; height:100%; object-fit:contain; }
.file-card-name {
  color:rgba(255,255,255,0.7); font-size:12px;
  overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
  flex:1; min-width:0;
}

/* 统一滚动条 — 滑块始终可见 */
.file-scroll,
.drop-zones {
  overflow-y:auto;
}
.file-scroll::-webkit-scrollbar,
.drop-zones::-webkit-scrollbar { width:5px; }
.file-scroll::-webkit-scrollbar-track,
.drop-zones::-webkit-scrollbar-track {
  background:transparent;
}
.file-scroll::-webkit-scrollbar-thumb,
.drop-zones::-webkit-scrollbar-thumb {
  background:rgba(255,255,255,0.25);
  border-radius:10px;
  border:1px solid rgba(255,255,255,0.05);
}
.file-scroll::-webkit-scrollbar-thumb:hover,
.drop-zones::-webkit-scrollbar-thumb:hover {
  background:rgba(255,255,255,0.35);
}

/* 右侧：放置区（卡片样式通过 .assign-categories 统一） */
.drop-zone {
  display:flex; align-items:center; gap:6px;
  padding:8px 10px; border-radius:10px;
  background:rgba(255,255,255,0.03);
  border:1.5px dashed rgba(255,255,255,0.08);
  border-left:3px solid var(--accent);
  transition:all 0.2s;
  min-height:48px;
}
.drop-zone:hover { background:rgba(255,255,255,0.06); border-color:rgba(255,255,255,0.15); }
.drop-zone.drag-over { background:color-mix(in srgb, var(--accent) 15%, transparent); border-color:var(--accent); }
.drop-zone-icon { font-size:14px; }
.drop-zone-label { color:rgba(255,255,255,0.75); font-size:12px; font-weight:600; flex:1; }
.drop-zone-label.custom-name { cursor:pointer; border-bottom:1px dashed rgba(255,255,255,0.15); }
.drop-zone-input {
  flex:1; background:rgba(255,255,255,0.06); border:1px solid rgba(255,255,255,0.15);
  border-radius:4px; color:white; font-size:11px; padding:1px 5px; outline:none; min-width:0;
}
.drop-zone-count { color:rgba(255,255,255,0.3); font-size:10px; white-space:nowrap; }
.drop-zone-del {
  width:20px; height:20px; border-radius:4px; border:none;
  background:transparent; color:rgba(255,255,255,0.2); font-size:10px;
  cursor:pointer; transition:all 0.15s; flex-shrink:0;
}
.drop-zone-del:hover { background:rgba(232,17,35,0.3); color:white; }
/* 拖拽幽灵 */
.drag-ghost {
  position:fixed; z-index:9999; pointer-events:none;
  transform:translate(-50%,-50%) scale(1.08);
  display:flex; align-items:center; gap:8px;
  padding:6px 14px 6px 10px;
  background:rgba(20,20,28,0.85);
  backdrop-filter:blur(16px); -webkit-backdrop-filter:blur(16px);
  border:1px solid rgba(255,255,255,0.12);
  border-radius:10px;
  box-shadow:0 8px 32px rgba(0,0,0,0.5);
}
.drag-ghost-icon { width:28px; height:28px; display:flex; align-items:center; justify-content:center; }
.drag-ghost-icon img { width:100%; height:100%; object-fit:contain; }
.drag-ghost-name { color:white; font-size:12px; white-space:nowrap; }

.add-zone-btn {
  padding:8px; border:1px dashed rgba(255,255,255,0.1); border-radius:10px;
  background:transparent; color:rgba(255,255,255,0.3); font-size:11px; cursor:pointer;
  transition:all 0.15s; text-align:center; margin-top:2px;
}
.add-zone-btn:hover { border-color:rgba(255,255,255,0.2); color:white; background:rgba(255,255,255,0.03); }

/* 设置面板弹出动画 */
.settings-popup-enter-active { transition:all 0.25s cubic-bezier(0.22,1,0.36,1); }
.settings-popup-leave-active { transition:all 0.15s ease; }
.settings-popup-enter-from { opacity:0; transform:scale(0.85); }
.settings-popup-leave-to { opacity:0; transform:scale(0.85); }

/* ---- 桌面区域 ---- */
.desktop-area {
  width:100%; height:100%; overflow:hidden;
  padding:48px 0 20px;
}
.desktop-area.has-dock { padding-bottom:80px; }

.loading-indicator {
  position:fixed; top:50%; left:50%; transform:translate(-50%,-50%);
  color:white; background:rgba(0,0,0,0.7); padding:10px 20px;
  border-radius:20px; z-index:1000;
}

/* ======== 轮播 ======== */
.carousel-wrapper {
  display:flex; align-items:center;
  height:100%; position:relative;
}
.carousel-viewport { flex:1; overflow:hidden; height:320px; margin:0 4px; display:flex; justify-content:center; }
.carousel-track {
  display:flex; gap:16px;
  height:100%; align-items:center;
}

/* ---- 卡片（伪 3D 滚轮） ---- */
.group-card {
  flex:0 0 220px; height:280px;
  background:rgba(30,30,40,0.55);
  backdrop-filter:blur(20px); -webkit-backdrop-filter:blur(20px);
  border:1px solid rgba(255,255,255,0.06);
  border-radius:16px; padding:14px;
  cursor:pointer; display:flex; flex-direction:column; align-items:center;
  transition:transform 0.5s cubic-bezier(0.25,0.8,0.25,1),
              opacity 0.5s ease;
  will-change:transform, opacity;
  box-shadow:0 4px 20px rgba(0,0,0,0.25);
}
.group-card:hover {
  border-color:var(--accent);
  box-shadow:0 8px 32px rgba(0,0,0,0.35);
}

/* 居中圆角标签 - 液态玻璃 */
.card-label {
  display:inline-flex; align-items:center;
  padding:5px 16px;
  border-radius:20px;
  backdrop-filter:blur(12px); -webkit-backdrop-filter:blur(12px);
  border:1px solid rgba(255,255,255,0.08);
  color:rgba(255,255,255,0.9);
  font-size:13px; font-weight:600;
  margin-bottom:14px;
  box-shadow:0 2px 12px rgba(0,0,0,0.2);
}

/* 2×2 预览 */
.card-preview {
  display:grid; grid-template-columns:1fr 1fr; gap:8px;
  width:100%; flex:1; align-content:center;
}
.preview-icon {
  width:100%; aspect-ratio:1;
  display:flex; align-items:center; justify-content:center;
  background:rgba(255,255,255,0.04);
  border-radius:10px; overflow:hidden;
}
.preview-icon img { width:100%; height:100%; object-fit:contain; padding:6px; }
.preview-more {
  display:flex; align-items:center; justify-content:center;
  color:rgba(255,255,255,0.4); font-size:13px;
  background:rgba(255,255,255,0.04); border-radius:10px;
}

/* ---- 箭头 ---- */
.carousel-arrow {
  position:absolute; top:50%; transform:translateY(-50%);
  z-index:10; width:44px; height:100px;
  display:flex; align-items:center; justify-content:center;
  background:rgba(25,25,35,0.5);
  backdrop-filter:blur(12px); -webkit-backdrop-filter:blur(12px);
  border:1px solid rgba(255,255,255,0.06);
  color:rgba(255,255,255,0.6); font-size:30px; font-weight:300;
  cursor:pointer; transition:all 0.2s; user-select:none;
}
.carousel-arrow.left { left:0; border-radius:0 14px 14px 0; border-left:none; }
.carousel-arrow.right { right:0; border-radius:14px 0 0 14px; border-right:none; }
.carousel-arrow:hover { background:rgba(255,255,255,0.12); color:white; transform:translateY(-50%) scaleX(1.1); }

/* ---- 展开 ---- */
.expanded-view { height:100%; display:flex; align-items:center; justify-content:center; padding:20px; }
.expanded-panel {
  width:100%; max-width:1000px; max-height:100%;
  background:rgba(30,30,40,0.6);
  backdrop-filter:blur(20px); -webkit-backdrop-filter:blur(20px);
  border:1px solid rgba(255,255,255,0.08); border-radius:20px;
  padding:24px 28px; overflow-y:auto;
  box-shadow:0 8px 40px rgba(0,0,0,0.35);
}
.expanded-header {
  display:flex; align-items:center; gap:12px;
  padding-bottom:16px; border-bottom:2px solid var(--accent); margin-bottom:20px;
}
.back-btn {
  background:rgba(255,255,255,0.06); border:none; color:rgba(255,255,255,0.7);
  padding:6px 14px; border-radius:8px; cursor:pointer; font-size:13px;
}
.back-btn:hover { background:rgba(255,255,255,0.15); color:white; }
.expanded-icon { font-size:24px; }
.expanded-title { color:white; font-size:18px; font-weight:700; }
.expanded-count { margin-left:auto; color:rgba(255,255,255,0.35); font-size:13px; }
.expanded-carousel {
  display:flex; align-items:center; gap:8px;
  height:220px; position:relative; overflow:hidden;
}
.expanded-track {
  flex:1; display:flex; justify-content:center; align-items:center;
  overflow:hidden; height:100%; position:relative;
}
.expanded-item-card {
  position:absolute;
  display:flex; flex-direction:column; align-items:center; gap:6px;
  cursor:pointer;
  transition:transform 0.5s cubic-bezier(0.25,0.8,0.25,1),
              opacity 0.5s ease;
  will-change:transform,opacity;
}
.expanded-item-icon {
  width:72px; height:72px; border-radius:14px;
  display:flex; align-items:center; justify-content:center;
  background:rgba(255,255,255,0.05);
  border:1px solid rgba(255,255,255,0.06);
  overflow:hidden;
}
.expanded-item-icon img { width:100%; height:100%; object-fit:contain; padding:8px; }
.expanded-item-name {
  color:rgba(255,255,255,0.8); font-size:11px;
  max-width:80px; text-align:center; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;
  text-shadow:0 1px 4px rgba(0,0,0,0.5);
}
.left-sm, .right-sm {
  width:32px; height:60px; flex-shrink:0;
  display:flex; align-items:center; justify-content:center;
  background:rgba(255,255,255,0.06);
  backdrop-filter:blur(8px); -webkit-backdrop-filter:blur(8px);
  border:1px solid rgba(255,255,255,0.06);
  border-radius:8px;
  color:rgba(255,255,255,0.5); font-size:24px;
  cursor:pointer; transition:all 0.2s; user-select:none;
}
.left-sm:hover, .right-sm:hover { background:rgba(255,255,255,0.12); color:white; }

.view-switch-enter-active { transition:all 0.3s ease; }
.view-switch-leave-active { transition:all 0.2s ease; }
.view-switch-enter-from { opacity:0; transform:scale(0.96); }
.view-switch-leave-to { opacity:0; transform:scale(0.96); }

/* ---- 手势滚轮提示 ---- */
.gesture-hint {
  position:fixed; bottom:70px; left:50%; transform:translateX(-50%);
  z-index:9999; pointer-events:none;
  padding:6px 16px;
  background:rgba(0,0,0,0.6);
  backdrop-filter:blur(8px); -webkit-backdrop-filter:blur(8px);
  border-radius:20px;
  color:rgba(255,255,255,0.85);
  font-size:13px;
  white-space:nowrap;
  animation:gestureFade 0.3s ease;
}
@keyframes gestureFade { from { opacity:0; transform:translateX(-50%) translateY(8px); } to { opacity:1; transform:translateX(-50%) translateY(0); } }

/* ---- 退出 ---- */
.exit-btn {
  position:fixed; bottom:14px; left:14px; z-index:1000;
  display:flex; align-items:center; gap:6px;
  padding:8px 14px; border:1px solid rgba(255,255,255,0.15); border-radius:10px;
  background:rgba(30,30,40,0.6);
  backdrop-filter:blur(12px); -webkit-backdrop-filter:blur(12px);
  color:rgba(255,255,255,0.7); font-size:12px; cursor:pointer; transition:all 0.2s;
}
.exit-btn:hover { background:rgba(232,17,35,0.5); border-color:rgba(232,17,35,0.4); color:white; }

/* ---- Dock ---- */
.dock {
  position:fixed; bottom:12px; left:50%; transform:translateX(-50%);
  z-index:900;
  background:rgba(30,30,40,0.75);
  backdrop-filter:blur(16px); -webkit-backdrop-filter:blur(16px);
  border:1px solid rgba(255,255,255,0.1); border-radius:16px;
  padding:8px 12px; box-shadow:0 4px 24px rgba(0,0,0,0.4);
}
.dock-inner { display:flex; align-items:flex-end; gap:4px; }
.dock-item {
  position:relative; display:flex; flex-direction:column; align-items:center;
  cursor:pointer; padding:6px 10px; border-radius:10px;
  transition:all 0.15s; min-width:56px;
}
.dock-item:hover { background:rgba(255,255,255,0.12); transform:translateY(-4px) scale(1.05); }
.dock-icon { width:40px; height:40px; display:flex; align-items:center; justify-content:center; }
.dock-icon img { width:100%; height:100%; object-fit:contain; }
.dock-label { color:rgba(255,255,255,0.8); font-size:10px; margin-top:4px; white-space:nowrap; max-width:60px; overflow:hidden; text-overflow:ellipsis; }
.dock-tooltip {
  position:absolute; bottom:100%; left:50%; transform:translateX(-50%) translateY(-8px);
  background:rgba(0,0,0,0.85); color:white; font-size:12px; padding:4px 10px;
  border-radius:6px; white-space:nowrap; pointer-events:none; opacity:0; transition:opacity 0.15s;
}
.dock-item:hover .dock-tooltip { opacity:1; }

/* ---- 摄像头 ---- */
.camera-floating {
  position:fixed; bottom:80px; right:20px; width:200px; height:150px;
  border-radius:12px; overflow:hidden;
  box-shadow:0 4px 12px rgba(0,0,0,0.3);
  border:2px solid rgba(255,255,255,0.3); background:#000; z-index:999;
}
.camera-floating video, .camera-floating canvas { position:absolute; top:0; left:0; width:100%; height:100%; object-fit:cover; }
.camera-floating canvas { pointer-events:none; }
</style>
