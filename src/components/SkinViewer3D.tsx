import { useEffect, useRef } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";

export type SkinPose = "idle" | "wave" | "run" | "sit" | "tpose";

/** 可单独隐藏的皮肤部位 */
export type SkinPartKey = "head" | "body" | "arms" | "legs";

interface SkinViewer3DProps {
  /** data URL 优先，其次远程 URL */
  dataUrl?: string;
  url?: string;
  capeDataUrl?: string;
  showCape?: boolean;
  /** 是否显示帽子/盔甲等外层（仅 64x64 皮肤有效） */
  showOverlay?: boolean;
  /** 实心：透明孔洞用取色填充，不透出内部 */
  solid?: boolean;
  /** 需要隐藏的部位 */
  hiddenParts?: SkinPartKey[];
  slim?: boolean;
  pose: SkinPose;
  autoRotate?: boolean;
}

type FaceRect = [number, number, number, number];

interface PartRects {
  right: FaceRect;
  left: FaceRect;
  top: FaceRect;
  bottom: FaceRect;
  front: FaceRect;
  back: FaceRect;
}

function faceCanvas(
  image: HTMLImageElement,
  rect: FaceRect,
  solid: boolean
): HTMLCanvasElement {
  const [sx, sy, sw, sh] = rect;
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, sw);
  canvas.height = Math.max(1, sh);
  const ctx = canvas.getContext("2d");
  if (!ctx) return canvas;
  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.drawImage(image, sx, sy, sw, sh, 0, 0, sw, sh);
  if (solid) {
    // 用该面主色填充透明像素，避免看到模型内部
    const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
    const data = imageData.data;
    let r = 0;
    let g = 0;
    let b = 0;
    let count = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] > 128) {
        r += data[i];
        g += data[i + 1];
        b += data[i + 2];
        count += 1;
      }
    }
    if (count > 0) {
      ctx.globalCompositeOperation = "destination-over";
      ctx.fillStyle = `rgb(${Math.round(r / count)},${Math.round(g / count)},${Math.round(b / count)})`;
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.globalCompositeOperation = "source-over";
    }
  }
  return canvas;
}

function textureFromCanvas(canvas: HTMLCanvasElement): THREE.CanvasTexture {
  const texture = new THREE.CanvasTexture(canvas);
  texture.magFilter = THREE.NearestFilter;
  texture.minFilter = THREE.NearestFilter;
  texture.colorSpace = THREE.SRGBColorSpace;
  return texture;
}

function partMaterials(
  image: HTMLImageElement,
  rects: PartRects,
  solid: boolean,
  doubleSide: boolean
): THREE.MeshStandardMaterial[] {
  const faces: FaceRect[] = [
    rects.right,
    rects.left,
    rects.top,
    rects.bottom,
    rects.front,
    rects.back,
  ];
  return faces.map(
    (face) =>
      new THREE.MeshStandardMaterial({
        map: textureFromCanvas(faceCanvas(image, face, solid && !doubleSide)),
        transparent: true,
        alphaTest: 0.05,
        side: doubleSide ? THREE.DoubleSide : THREE.FrontSide,
        roughness: 0.95,
        metalness: 0,
      })
  );
}

export function SkinViewer3D({
  dataUrl,
  url,
  capeDataUrl,
  showCape = true,
  showOverlay = true,
  solid = true,
  hiddenParts = [],
  slim,
  pose,
  autoRotate,
}: SkinViewer3DProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const poseRef = useRef<SkinPose>(pose);
  poseRef.current = pose;
  const hiddenRef = useRef<SkinPartKey[]>(hiddenParts);
  hiddenRef.current = hiddenParts;
  const visibilityHookRef = useRef<(() => void) | null>(null);

  // 部位隐藏变化时直接切换可见性，不重建场景
  useEffect(() => {
    visibilityHookRef.current?.();
  }, [hiddenParts]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;
    const source = dataUrl || url || "";

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(32, 1, 0.1, 500);
    camera.position.set(0, 12, 82);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    renderer.setClearColor(0x000000, 0);
    renderer.domElement.style.width = "100%";
    renderer.domElement.style.height = "100%";
    renderer.domElement.style.display = "block";
    host.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.target.set(0, 11, 0);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    controls.enablePan = false;
    controls.minDistance = 34;
    controls.maxDistance = 160;
    controls.autoRotate = Boolean(autoRotate);
    controls.autoRotateSpeed = 1.8;

    scene.add(new THREE.AmbientLight(0xffffff, 1.7));
    const key = new THREE.DirectionalLight(0xffffff, 2.1);
    key.position.set(20, 40, 32);
    scene.add(key);
    const fill = new THREE.DirectionalLight(0x8fb8ff, 0.9);
    fill.position.set(-30, 10, -20);
    scene.add(fill);

    const root = new THREE.Group();
    scene.add(root);

    const fitCamera = () => {
      const box = new THREE.Box3().setFromObject(root);
      if (box.isEmpty()) return;
      const size = box.getSize(new THREE.Vector3());
      const center = box.getCenter(new THREE.Vector3());
      const width = host.clientWidth || 320;
      const height = host.clientHeight || 430;
      camera.aspect = width / Math.max(height, 1);
      camera.updateProjectionMatrix();
      const fov = THREE.MathUtils.degToRad(camera.fov);
      const fitHeight = size.y / (2 * Math.tan(fov / 2));
      const fitWidth = size.x / (2 * Math.tan(fov / 2) * camera.aspect);
      const distance = Math.max(fitHeight, fitWidth) * 1.22;
      const targetY = center.y + size.y * 0.07;
      camera.position.set(center.x, targetY, center.z + distance);
      controls.target.set(center.x, targetY, center.z);
      controls.update();
    };

    const geometryCache: THREE.BoxGeometry[] = [];
    const materialCache: THREE.MeshStandardMaterial[] = [];

    const makeBox = (
      width: number,
      height: number,
      depth: number,
      materials: THREE.MeshStandardMaterial[]
    ) => {
      const geometry = new THREE.BoxGeometry(width, height, depth);
      geometryCache.push(geometry);
      materialCache.push(...materials);
      return new THREE.Mesh(geometry, materials);
    };

    const image = new Image();
    image.crossOrigin = "anonymous";
    image.onload = () => {
      if (disposed) return;
      const armWidth = slim ? 3 : 4;
      const armX = 4 + armWidth / 2;
      const is64 = image.height >= 64;
      const overlayOn = showOverlay && is64;

      interface PartEntry {
        key: SkinPartKey;
        group: THREE.Group;
        base: THREE.Mesh;
        overlay?: THREE.Mesh;
      }
      const parts: PartEntry[] = [];

      const buildPart = (
        key: SkinPartKey,
        group: THREE.Group,
        width: number,
        height: number,
        depth: number,
        baseRects: PartRects,
        overlayRects?: PartRects,
        meshOffsetY = 0
      ) => {
        const base = makeBox(width, height, depth, partMaterials(image, baseRects, solid, false));
        base.position.y = meshOffsetY;
        group.add(base);
        let overlay: THREE.Mesh | undefined;
        if (overlayOn && overlayRects) {
          // 外层只需比基础层略大，避免 z-fighting 即可。
          // 不能按比例放大：BoxGeometry 的 UV 覆盖整面，放大盒子会把纹理像素一起拉伸，
          // 导致外层（头发/帽子）与基础层像素错位。固定微小膨胀保持两层像素 1:1 对齐。
          const OVERLAY_INFLATE = 0.08;
          overlay = makeBox(
            width + OVERLAY_INFLATE,
            height + OVERLAY_INFLATE,
            depth + OVERLAY_INFLATE,
            partMaterials(image, overlayRects, false, !solid)
          );
          overlay.position.y = meshOffsetY;
          group.add(overlay);
        }
        parts.push({ key, group, base, overlay });
      };

      const head = new THREE.Group();
      head.position.set(0, 24, 0);
      buildPart(
        "head",
        head,
        8,
        8,
        8,
        {
          right: [0, 8, 8, 8],
          left: [16, 8, 8, 8],
          top: [8, 0, 8, 8],
          bottom: [16, 0, 8, 8],
          front: [8, 8, 8, 8],
          back: [24, 8, 8, 8],
        },
        {
          right: [32, 8, 8, 8],
          left: [48, 8, 8, 8],
          top: [40, 0, 8, 8],
          bottom: [48, 0, 8, 8],
          front: [40, 8, 8, 8],
          back: [56, 8, 8, 8],
        },
        4
      );
      root.add(head);

      const body = new THREE.Group();
      body.position.set(0, 18, 0);
      buildPart(
        "body",
        body,
        8,
        12,
        4,
        {
          right: [16, 20, 4, 12],
          left: [28, 20, 4, 12],
          top: [20, 16, 8, 4],
          bottom: [28, 16, 8, 4],
          front: [20, 20, 8, 12],
          back: [32, 20, 8, 12],
        },
        {
          right: [32, 36, 4, 12],
          left: [44, 36, 4, 12],
          top: [40, 32, 8, 4],
          bottom: [48, 32, 8, 4],
          front: [40, 36, 8, 12],
          back: [52, 36, 8, 12],
        }
      );
      root.add(body);

      const rightArm = new THREE.Group();
      rightArm.position.set(armX, 24, 0);
      buildPart(
        "arms",
        rightArm,
        armWidth,
        12,
        4,
        {
          right: [40, 20, 4, 12],
          left: [48, 20, 4, 12],
          top: [44, 16, 4, 4],
          bottom: [48, 16, 4, 4],
          front: [44, 20, 4, 12],
          back: [52, 20, 4, 12],
        },
        {
          right: [40, 52, 4, 12],
          left: [48, 52, 4, 12],
          top: [44, 48, 4, 4],
          bottom: [48, 48, 4, 4],
          front: [44, 52, 4, 12],
          back: [52, 52, 4, 12],
        },
        -6
      );
      root.add(rightArm);

      const leftArm = new THREE.Group();
      leftArm.position.set(-armX, 24, 0);
      buildPart(
        "arms",
        leftArm,
        armWidth,
        12,
        4,
        {
          right: [32, 52, 4, 12],
          left: [40, 52, 4, 12],
          top: [36, 48, 4, 4],
          bottom: [40, 48, 4, 4],
          front: [36, 52, 4, 12],
          back: [44, 52, 4, 12],
        },
        is64
          ? {
              right: [48, 52, 4, 12],
              left: [56, 52, 4, 12],
              top: [52, 48, 4, 4],
              bottom: [56, 48, 4, 4],
              front: [52, 52, 4, 12],
              back: [60, 52, 4, 12],
            }
          : undefined,
        -6
      );
      root.add(leftArm);

      const rightLeg = new THREE.Group();
      rightLeg.position.set(2, 12, 0);
      buildPart(
        "legs",
        rightLeg,
        4,
        12,
        4,
        {
          right: [0, 20, 4, 12],
          left: [8, 20, 4, 12],
          top: [4, 16, 4, 4],
          bottom: [8, 16, 4, 4],
          front: [4, 20, 4, 12],
          back: [12, 20, 4, 12],
        },
        is64
          ? {
              right: [0, 36, 4, 12],
              left: [8, 36, 4, 12],
              top: [4, 32, 4, 4],
              bottom: [8, 32, 4, 4],
              front: [4, 36, 4, 12],
              back: [12, 36, 4, 12],
            }
          : undefined,
        -6
      );
      root.add(rightLeg);

      const leftLeg = new THREE.Group();
      leftLeg.position.set(-2, 12, 0);
      buildPart(
        "legs",
        leftLeg,
        4,
        12,
        4,
        {
          right: [16, 52, 4, 12],
          left: [24, 52, 4, 12],
          top: [20, 48, 4, 4],
          bottom: [24, 48, 4, 4],
          front: [20, 52, 4, 12],
          back: [28, 52, 4, 12],
        },
        is64
          ? {
              // 64x64 左腿外层在贴图 [0,48]-[16,64] 区域，不能复用基础层 UV
              right: [0, 52, 4, 12],
              left: [8, 52, 4, 12],
              top: [4, 48, 4, 4],
              bottom: [8, 48, 4, 4],
              front: [4, 52, 4, 12],
              back: [12, 52, 4, 12],
            }
          : undefined,
        -6
      );
      root.add(leftLeg);

      const applyVisibility = () => {
        const hidden = new Set(hiddenRef.current);
        for (const part of parts) {
          part.group.visible = !hidden.has(part.key);
        }
      };
      applyVisibility();
      visibilityHookRef.current = applyVisibility;
      fitCamera();

      let capeGroup: THREE.Group | null = null;
      if (showCape && capeDataUrl) {
        const capeImage = new Image();
        capeImage.crossOrigin = "anonymous";
        capeImage.onload = () => {
          if (disposed) return;
          const capeMesh = makeBox(
            10,
            16,
            1,
            partMaterials(capeImage, {
              // 装饰面朝向玩家背后（-z），素面朝向身体
              right: [0, 1, 1, 16],
              left: [11, 1, 1, 16],
              top: [1, 0, 10, 1],
              bottom: [11, 0, 10, 1],
              front: [12, 1, 10, 16],
              back: [1, 1, 10, 16],
            }, solid, false)
          );
          capeMesh.position.y = -8;
          capeGroup = new THREE.Group();
          capeGroup.position.set(0, 24, -2.6);
          capeGroup.add(capeMesh);
          root.add(capeGroup);
        };
        capeImage.src = capeDataUrl;
      }

      let frame = 0;
      const animate = () => {
        if (disposed) return;
        frame += 1;
        const time = performance.now() / 1000;
        const current = poseRef.current;
        head.rotation.set(0, 0, 0);
        rightArm.rotation.set(0, 0, 0);
        leftArm.rotation.set(0, 0, 0);
        rightLeg.rotation.set(0, 0, 0);
        leftLeg.rotation.set(0, 0, 0);
        root.position.y = 0;
        if (capeGroup) {
          capeGroup.rotation.x =
            current === "run" ? -0.35 - Math.abs(Math.sin(time * 8)) * 0.18 : -0.04;
        }

        if (current === "wave") {
          rightArm.rotation.z = -2.25 + Math.sin(time * 4) * 0.18;
          head.rotation.z = -0.08;
        } else if (current === "run") {
          const swing = Math.sin(time * 8) * 0.85;
          rightArm.rotation.x = swing;
          leftArm.rotation.x = -swing;
          rightLeg.rotation.x = -swing * 0.9;
          leftLeg.rotation.x = swing * 0.9;
          root.position.y = Math.abs(Math.sin(time * 8)) * 0.8;
        } else if (current === "sit") {
          rightLeg.rotation.x = -1.45;
          leftLeg.rotation.x = -1.45;
          rightArm.rotation.x = -0.28;
          leftArm.rotation.x = -0.28;
          root.position.y = -3;
        } else if (current === "tpose") {
          rightArm.rotation.z = Math.PI / 2;
          leftArm.rotation.z = -Math.PI / 2;
        }

        controls.update();
        renderer.render(scene, camera);
        if (frame > 0) requestAnimationFrame(animate);
      };
      animate();
    };
    image.src = source;

    const resize = () => {
      const width = host.clientWidth || 320;
      const height = host.clientHeight || 380;
      renderer.setSize(width, height, false);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(host);

    return () => {
      disposed = true;
      observer.disconnect();
      visibilityHookRef.current = null;
      controls.dispose();
      geometryCache.forEach((geometry) => geometry.dispose());
      materialCache.forEach((material) => {
        material.map?.dispose();
        material.dispose();
      });
      renderer.dispose();
      renderer.domElement.remove();
    };
  }, [dataUrl, url, capeDataUrl, showCape, showOverlay, solid, slim, autoRotate]);

  return <div ref={hostRef} className="skin-viewer-3d" />;
}
