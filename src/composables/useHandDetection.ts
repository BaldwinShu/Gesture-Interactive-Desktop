// src/composables/useHandDetection.ts
import * as handPoseDetection from '@tensorflow-models/hand-pose-detection';
import '@mediapipe/hands';

export interface GestureResult {
  isPointing: boolean;
  isOpenPalm: boolean;
  isFist: boolean;
  isGrabPose: boolean;
  isMousePose: boolean;
  thumbStraight: boolean;
  handedness: 'Left' | 'Right' | 'Unknown';
  palmX: number;
  palmY: number;
  indexTipX: number;
  indexTipY: number;
  indexZ: number;
}

function fingerState(
  k: handPoseDetection.Keypoint[],
  tip: number, pip: number,
): 'straight' | 'bent' | 'unknown' {
  if (!k[tip] || !k[pip]) return 'unknown';
  return k[tip].y < k[pip].y ? 'straight' : 'bent';
}

class FrameBuffer {
  private buf: GestureResult[] = [];
  constructor(private size: number) {}
  push(r: GestureResult) {
    this.buf.push(r);
    if (this.buf.length > this.size) this.buf.shift();
  }
  stable(pred: (r: GestureResult) => boolean): boolean {
    if (this.buf.length < 2) return false;
    return this.buf.filter(pred).length >= Math.ceil(this.buf.length * 0.6);
  }
  clear() { this.buf = []; }
}

export async function initHandDetection(videoElement: HTMLVideoElement) {
  const model = handPoseDetection.SupportedModels.MediaPipeHands;
  const detectorConfig: handPoseDetection.MediaPipeHandsMediaPipeModelConfig = {
    runtime: 'mediapipe',
    solutionPath: '/mediapipe',
    modelType: 'full',
    maxHands: 2,
  };
  const detector = await handPoseDetection.createDetector(model, detectorConfig);

  const stream = await navigator.mediaDevices.getUserMedia({ video: true });
  videoElement.srcObject = stream;
  await videoElement.play();

  const buf = new FrameBuffer(5);

  async function detectHands(): Promise<handPoseDetection.Hand[]> {
    if (videoElement.readyState < 2) return [];
    return detector.estimateHands(videoElement);
  }

  function analyzeGesture(hands: handPoseDetection.Hand[]): {
    results: GestureResult[];
    anyPointing: boolean;
    anyOpenPalm: boolean;
    anyFist: boolean;
    anyMousePose: boolean;
    count: number;
    handDist: number;
    avgPalmX: number;
    avgPalmY: number;
  } {
    const results: GestureResult[] = [];
    const vw = videoElement.videoWidth || 640;
    const vh = videoElement.videoHeight || 480;
    const imgSize = Math.max(vw, vh);

    for (const hand of hands) {
      const k = hand.keypoints;
      const palmX = k[9]?.x ?? 0;
      const palmY = k[9]?.y ?? 0;
      const dx = (k[0]?.x ?? 0) - (k[9]?.x ?? 0);
      const dy = (k[0]?.y ?? 0) - (k[9]?.y ?? 0);
      const spanNorm = Math.hypot(dx, dy) / imgSize;

      const idx = fingerState(k, 8, 6);
      const thumb = fingerState(k, 4, 3);
      const mid = fingerState(k, 12, 10);
      const ring = fingerState(k, 16, 14);
      const pinky = fingerState(k, 20, 18);

      // 鼠标姿势：用指尖→掌心距离判断（不受手指朝向影响）
      const handSize = Math.hypot(k[0]?.x - k[9]?.x || 0, k[0]?.y - k[9]?.y || 0);
      const idxExt = Math.hypot(k[8]?.x - k[9]?.x || 0, k[8]?.y - k[9]?.y || 0) > handSize * 0.5;
      const midExt = Math.hypot(k[12]?.x - k[9]?.x || 0, k[12]?.y - k[9]?.y || 0) > handSize * 0.5;
      const ringBent = Math.hypot(k[16]?.x - k[9]?.x || 0, k[16]?.y - k[9]?.y || 0) < handSize * 0.55;
      const pinkyBent = Math.hypot(k[20]?.x - k[9]?.x || 0, k[20]?.y - k[9]?.y || 0) < handSize * 0.55;
      const together = k[8] && k[12] && Math.abs(k[8].x - k[12].x) < handSize * 0.25;
      const mousePose = idxExt && midExt && together && ringBent && pinkyBent;
      if (palmX > 0) console.log('[Pose]', { idxExt, midExt, together, ringBent, pinkyBent, hs: handSize.toFixed(1), mp: mousePose });
      const handedness: 'Left' | 'Right' | 'Unknown' =
        (hand as any).handedness === 'Left' ? 'Left'
        : (hand as any).handedness === 'Right' ? 'Right' : 'Unknown';

      results.push({
        isPointing: idx === 'straight' && mid === 'bent' && ring === 'bent' && pinky === 'bent',
        isOpenPalm: [idx, thumb, mid, ring, pinky].filter(s => s === 'straight').length >= 4,
        isFist: [idx, thumb, mid, ring, pinky].filter(s => s === 'bent').length >= 4,
        isGrabPose: mousePose && k[4] && k[16] && Math.hypot(k[4].x - k[16].x, k[4].y - k[16].y) < 0.06,
        isMousePose: mousePose,
        thumbStraight: thumb === 'straight',
        handedness,
        palmX,
        palmY,
        indexTipX: k[8]?.x ?? 0,
        indexTipY: k[8]?.y ?? 0,
        indexZ: spanNorm,
      });
    }

    buf.push(results[0] || {
      isPointing: false, isOpenPalm: false, isFist: false, isGrabPose: false,
      isMousePose: false, thumbStraight: false, handedness: 'Unknown' as const,
      palmX: 0, palmY: 0, indexTipX: 0, indexTipY: 0, indexZ: 0,
    });

    const stablePointing = buf.stable(r => r.isPointing);
    const stableOpenPalm = buf.stable(r => r.isOpenPalm);
    const stableFist = buf.stable(r => r.isFist);
    const stableMouse = buf.stable(r => r.isMousePose);
    const dist = (results[0]?.indexZ ?? 0) * 1000;
    const sumX = results.reduce((s, r) => s + r.palmX, 0);
    const sumY = results.reduce((s, r) => s + r.palmY, 0);
    const n = Math.max(1, results.length);

    return {
      results,
      anyPointing: stablePointing,
      anyOpenPalm: stableOpenPalm,
      anyFist: stableFist,
      anyMousePose: stableMouse,
      count: hands.length,
      handDist: dist,
      avgPalmX: sumX / n,
      avgPalmY: sumY / n,
    };
  }

  async function stopCamera() {
    const s = videoElement.srcObject as MediaStream;
    if (s) { s.getTracks().forEach(t => t.stop()); videoElement.srcObject = null; }
  }

  return { detectHands, analyzeGesture, stopCamera, clearBuffer: () => buf.clear() };
}
