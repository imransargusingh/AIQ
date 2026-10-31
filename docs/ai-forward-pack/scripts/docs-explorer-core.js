/**
 * Deterministic state, traversal, search, and layout core for the Docs Explorer.
 */
(function (root, factory) {
  const api = factory();
  if (typeof module === "object" && module.exports) module.exports = api;
  root.DocsExplorerCore = api;
})(typeof globalThis !== "undefined" ? globalThis : this, function () {
  "use strict";

  const PROJECTIONS = new Set(["browse", "graph", "mindmap", "spatial3d"]);
  const ROUTES = new Set(["browse", "visualization", "details"]);
  const PATH_MODES = new Set(["none", "grounding", "impact", "proof"]);

  function sortedUnique(values) {
    return [...new Set((values || []).filter(Boolean).map(String))].sort();
  }

  function canonicalJson(value) {
    // Public for the cross-runtime Python/JavaScript grounding hash contract.
    if (value === null || typeof value !== "object") return JSON.stringify(value);
    if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
    return `{${Object.keys(value).sort().map(
      (key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`,
    ).join(",")}}`;
  }

  async function runWithDeadline(operation, timeoutMs) {
    if (typeof operation !== "function") throw new TypeError("operation must be a function");
    if (!Number.isFinite(timeoutMs) || timeoutMs < 0) {
      throw new RangeError("timeoutMs must be a non-negative finite number");
    }
    const controller = new AbortController();
    let timeoutId;
    const timeout = new Promise((_, reject) => {
      timeoutId = setTimeout(() => {
        const error = new Error("operation deadline exceeded");
        error.name = "TimeoutError";
        controller.abort(error);
        reject(error);
      }, timeoutMs);
    });
    try {
      return await Promise.race([
        Promise.resolve().then(() => operation(controller.signal)),
        timeout,
      ]);
    } finally {
      clearTimeout(timeoutId);
    }
  }

  function initialState() {
    return {
      projection: "browse",
      route: "browse",
      selectedNodeId: null,
      contextNodeId: null,
      filters: { types: [], statuses: [], health: [], relations: [] },
      search: { query: "" },
      pathMode: "none",
      notice: "",
    };
  }

  function artifactIds(index) {
    return new Set((index.artifacts || []).map((artifact) => artifact.id));
  }

  function normalizeState(candidate, index, narrow, options) {
    const base = initialState();
    const ids = artifactIds(index);
    const requestedSelection = candidate?.selectedNodeId;
    const requestedContext = candidate?.contextNodeId;
    const state = {
      ...base,
      ...candidate,
      notice: options?.preserveNotice ? String(candidate?.notice || "") : "",
      filters: {
        types: sortedUnique(candidate?.filters?.types),
        statuses: sortedUnique(candidate?.filters?.statuses),
        health: sortedUnique(candidate?.filters?.health),
        relations: sortedUnique(candidate?.filters?.relations),
      },
      search: { ...base.search, ...(candidate?.search || {}) },
    };
    state.projection = PROJECTIONS.has(state.projection) ? state.projection : "browse";
    state.route = ROUTES.has(state.route) ? state.route : "browse";
    state.pathMode = PATH_MODES.has(state.pathMode) ? state.pathMode : "none";
    state.selectedNodeId = ids.has(state.selectedNodeId) ? state.selectedNodeId : null;
    state.contextNodeId = ids.has(state.contextNodeId) ? state.contextNodeId : null;
    if (requestedSelection && !state.selectedNodeId) {
      state.notice = "The selected artifact is no longer available. Returned to the project map.";
      state.route = "browse";
      state.projection = "browse";
    }
    if (requestedContext && !state.contextNodeId) {
      state.notice = "The neighborhood artifact is no longer available. Returned to project context.";
    }
    if (state.contextNodeId && !visibleArtifact(index, state.contextNodeId, state.filters)) {
      state.contextNodeId = null;
      state.pathMode = "none";
      state.notice = "Neighborhood context was cleared because filters exclude it.";
    }
    if (narrow) {
      if (state.route === "browse") state.projection = "browse";
      if (state.route === "visualization" && state.projection === "browse") {
        state.projection = "graph";
      }
    } else {
      state.route = state.projection === "browse" ? "browse" : "visualization";
    }
    return state;
  }

  function reduceState(state, event, index, options) {
    const ids = artifactIds(index);
    const narrow = Boolean(options?.narrow);
    let next = { ...state, notice: "" };
    switch (event.type) {
      case "SELECT_NODE":
        if (!ids.has(event.id)) return { ...state, notice: "Artifact is unavailable." };
        next.selectedNodeId = event.id;
        break;
      case "EXPLORE_CONTEXT":
        if (!ids.has(event.id) || !visibleArtifact(index, event.id, state.filters)) {
          return { ...state, notice: "Context is unavailable in the current filters." };
        }
        next.contextNodeId = event.id;
        next.route = "visualization";
        if (next.projection === "browse") next.projection = "graph";
        break;
      case "LEAVE_CONTEXT":
        next.contextNodeId = null;
        if (next.pathMode !== "none") next.pathMode = "none";
        break;
      case "SET_PROJECTION":
        next.projection = PROJECTIONS.has(event.value) ? event.value : "browse";
        if (narrow) next.route = next.projection === "browse" ? "browse" : "visualization";
        break;
      case "SET_ROUTE":
        next.route = ROUTES.has(event.value) ? event.value : "browse";
        if (narrow && next.route === "browse") next.projection = "browse";
        if (narrow && next.route === "visualization" && next.projection === "browse") {
          next.projection = "graph";
        }
        break;
      case "SET_SEARCH":
        next.search = { query: String(event.query || "") };
        break;
      case "APPLY_FILTERS":
        next.filters = {
          types: sortedUnique(event.filters?.types),
          statuses: sortedUnique(event.filters?.statuses),
          health: sortedUnique(event.filters?.health),
          relations: sortedUnique(event.filters?.relations),
        };
        if (next.contextNodeId && !visibleArtifact(index, next.contextNodeId, next.filters)) {
          next.contextNodeId = null;
          next.pathMode = "none";
          next.notice = "Neighborhood context was cleared because filters exclude it.";
        }
        break;
      case "SET_PATH_MODE":
        if (!PATH_MODES.has(event.value)) {
          return { ...state, notice: "Path mode is unavailable." };
        }
        next.pathMode = next.pathMode === event.value ? "none" : event.value;
        if (next.pathMode !== "none") {
          next.projection = "graph";
          if (narrow) next.route = "visualization";
        }
        break;
      case "RESTORE_STATE":
        return normalizeState(event.state || initialState(), index, narrow);
      default:
        return state;
    }
    return normalizeState(next, index, narrow, { preserveNotice: true });
  }

  function visibleArtifact(index, id, filters) {
    const artifact = (index.artifacts || []).find((item) => item.id === id);
    if (!artifact) return false;
    if (filters.types.length && !filters.types.includes(artifact.type)) return false;
    if (filters.statuses.length && !filters.statuses.includes(artifact.status)) return false;
    const health = artifactHealth(artifact);
    if (filters.health.length && !filters.health.some((item) => health.includes(item))) {
      return false;
    }
    return true;
  }

  function artifactHealth(artifact, today) {
    const result = [];
    const now = today || new Date().toISOString().slice(0, 10);
    if (artifact.reviewBy && artifact.reviewBy < now) result.push("stale");
    if ((artifact.reviewSuggested || []).length) result.push("review-suggested");
    return result.length ? result : ["healthy"];
  }

  function mindMapRoot(state, index) {
    if (state.contextNodeId) return state.contextNodeId;
    const ids = artifactIds(index);
    if (ids.has(index.rootId)) return index.rootId;
    return [...ids].sort()[0] || null;
  }

  function decodeUrlState(params, index, options) {
    const state = initialState();
    state.projection = params.get("view") || state.projection;
    state.route = params.get("route") || state.route;
    state.selectedNodeId = params.get("selected");
    state.contextNodeId = params.get("context");
    state.pathMode = params.get("path") || state.pathMode;
    state.filters = {
      types: params.getAll("type"),
      statuses: params.getAll("status"),
      health: params.getAll("health"),
      relations: params.getAll("relation"),
    };
    return normalizeState(state, index, Boolean(options?.narrow));
  }

  function encodeUrlState(state) {
    const params = new URLSearchParams();
    if (state.route !== "browse") params.set("route", state.route);
    if (state.projection !== "browse") params.set("view", state.projection);
    if (state.selectedNodeId) params.set("selected", state.selectedNodeId);
    if (state.contextNodeId) params.set("context", state.contextNodeId);
    if (state.pathMode !== "none") params.set("path", state.pathMode);
    const mappings = [
      ["type", state.filters.types],
      ["status", state.filters.statuses],
      ["health", state.filters.health],
      ["relation", state.filters.relations],
    ];
    for (const [key, values] of mappings) {
      for (const value of sortedUnique(values)) params.append(key, value);
    }
    params.sort();
    return params.toString();
  }

  function normalizeProjection(index, filters) {
    const appliedFilters = filters || initialState().filters;
    const typeRank = new Map(
      (index.artifactTypes || sortedUnique((index.artifacts || []).map((a) => a.type))).map(
        (type, rank) => [type, rank],
      ),
    );
    const allNodes = (index.artifacts || []).map((artifact) => ({
      ...artifact,
      lane: artifact.type || "other",
      laneRank: typeRank.has(artifact.type) ? typeRank.get(artifact.type) : 999,
      health: artifactHealth(artifact),
    }));
    const nodes = allNodes
      .filter((node) => visibleArtifact(index, node.id, appliedFilters))
      .sort((left, right) => left.laneRank - right.laneRank || left.id.localeCompare(right.id));
    const visibleIds = new Set(nodes.map((node) => node.id));
    const seen = new Set();
    const edges = [];
    for (const artifact of index.artifacts || []) {
      for (const link of artifact.links || []) {
        const id = `${artifact.id}|${link.rel}|${link.to}`;
        if (seen.has(id)) throw new Error(`duplicate typed link: ${id}`);
        seen.add(id);
        if (!visibleIds.has(artifact.id) || !visibleIds.has(link.to)) continue;
        if (appliedFilters.relations.length && !appliedFilters.relations.includes(link.rel)) {
          continue;
        }
        edges.push({ id, source: artifact.id, rel: link.rel, target: link.to });
      }
    }
    edges.sort(
      (left, right) =>
        left.source.localeCompare(right.source) ||
        left.rel.localeCompare(right.rel) ||
        left.target.localeCompare(right.target),
    );
    return { nodes, edges, typeRank };
  }

  function deterministicLayout(model, width, height) {
    const lanes = new Map();
    for (const node of model.nodes) {
      if (!lanes.has(node.lane)) lanes.set(node.lane, []);
      lanes.get(node.lane).push(node);
    }
    const laneEntries = [...lanes.entries()].sort((left, right) => {
      const leftRank = left[1][0]?.laneRank ?? 999;
      const rightRank = right[1][0]?.laneRank ?? 999;
      return leftRank - rightRank || left[0].localeCompare(right[0]);
    });
    const laneWidth = Math.max(180, width / Math.max(1, laneEntries.length));
    const positioned = [];
    laneEntries.forEach(([lane, nodes], laneIndex) => {
      const rowHeight = Math.max(72, (height - 80) / Math.max(1, nodes.length));
      nodes.forEach((node, rowIndex) => {
        positioned.push({
          ...node,
          x: laneIndex * laneWidth + laneWidth / 2,
          y: 70 + rowIndex * rowHeight,
          lane,
        });
      });
    });
    return {
      nodes: positioned,
      edges: model.edges,
      lanes: laneEntries.map(([lane], laneIndex) => ({
        id: lane,
        x: laneIndex * laneWidth,
        width: laneWidth,
      })),
      work: {
        nodeVisits: model.nodes.length + positioned.length,
        edgeVisits: model.edges.length,
      },
      width,
      height,
    };
  }

  function stableStringHash(value) {
    let hash = 2166136261;
    for (const character of String(value)) {
      hash ^= character.codePointAt(0);
      hash = Math.imul(hash, 16777619);
    }
    return hash >>> 0;
  }

  function deterministic3DLayout(model) {
    const lanes = new Map();
    for (const node of model.nodes || []) {
      const lane = node.lane || "other";
      if (!lanes.has(lane)) lanes.set(lane, []);
      lanes.get(lane).push(node);
    }
    const laneEntries = [...lanes.entries()].sort((left, right) => {
      const leftRank = Math.min(...left[1].map((node) => node.laneRank ?? 999));
      const rightRank = Math.min(...right[1].map((node) => node.laneRank ?? 999));
      return leftRank - rightRank || left[0].localeCompare(right[0]);
    });
    const positioned = [];
    laneEntries.forEach(([lane, laneNodes], laneIndex) => {
      const orderedNodes = [...laneNodes].sort((left, right) => left.id.localeCompare(right.id));
      const laneX = (laneIndex - (laneEntries.length - 1) / 2) * 260;
      const depthBand = ((laneIndex % 3) - 1) * 150;
      orderedNodes.forEach((node, rowIndex) => {
        const hashUnit = stableStringHash(node.id) / 0xffffffff;
        positioned.push({
          ...node,
          lane,
          x: laneX,
          y: (rowIndex - (orderedNodes.length - 1) / 2) * 120,
          z: depthBand + (hashUnit - 0.5) * 120,
        });
      });
    });
    positioned.sort((left, right) => left.id.localeCompare(right.id));
    const center = positioned.length
      ? positioned.reduce(
        (sum, node) => ({
          x: sum.x + node.x / positioned.length,
          y: sum.y + node.y / positioned.length,
          z: sum.z + node.z / positioned.length,
        }),
        { x: 0, y: 0, z: 0 },
      )
      : { x: 0, y: 0, z: 0 };
    const edges = [...(model.edges || [])].sort(
      (left, right) =>
        left.source.localeCompare(right.source) ||
        left.rel.localeCompare(right.rel) ||
        left.target.localeCompare(right.target),
    );
    return {
      nodes: positioned,
      edges,
      center,
      lanes: laneEntries.map(([lane], laneIndex) => ({
        id: lane,
        x: (laneIndex - (laneEntries.length - 1) / 2) * 260,
      })),
      work: {
        nodeVisits: (model.nodes || []).length + positioned.length,
        edgeVisits: edges.length,
      },
    };
  }

  function canonicalSpatialCamera(layout, selectedNodeId, contextNodeId) {
    const target =
      layout.nodes.find((node) => node.id === selectedNodeId) ||
      layout.nodes.find((node) => node.id === contextNodeId) ||
      layout.center ||
      { x: 0, y: 0, z: 0 };
    return {
      yaw: -0.55,
      pitch: -0.24,
      zoom: 1,
      distance: 1100,
      target,
    };
  }

  function projectSpatialPoint(point, camera, width, height) {
    const target = camera.target || { x: 0, y: 0, z: 0 };
    const dx = point.x - target.x;
    const dy = point.y - target.y;
    const dz = point.z - target.z;
    const cosYaw = Math.cos(camera.yaw || 0);
    const sinYaw = Math.sin(camera.yaw || 0);
    const yawX = dx * cosYaw - dz * sinYaw;
    const yawZ = dx * sinYaw + dz * cosYaw;
    const cosPitch = Math.cos(camera.pitch || 0);
    const sinPitch = Math.sin(camera.pitch || 0);
    const viewY = dy * cosPitch - yawZ * sinPitch;
    const viewZ = dy * sinPitch + yawZ * cosPitch;
    const zoom = Math.max(0.35, Math.min(3, Number(camera.zoom) || 1));
    const distance = Math.max(300, Number(camera.distance) || 1100) / zoom;
    const depth = distance - viewZ;
    const focalLength = Math.min(width, height) * 1.15;
    const scale = focalLength / Math.max(80, depth);
    return {
      screenX: width / 2 + yawX * scale,
      screenY: height / 2 + viewY * scale,
      depth,
      scale,
      visible: depth > 80,
    };
  }

  function projectSpatialLayout(layout, camera, width, height) {
    const projectedNodes = layout.nodes
      .map((node) => ({
        ...node,
        ...projectSpatialPoint(node, camera, width, height),
      }))
      .sort((left, right) => right.depth - left.depth || left.id.localeCompare(right.id));
    const projectedById = new Map(projectedNodes.map((node) => [node.id, node]));
    return {
      nodes: projectedNodes,
      edges: layout.edges.map((edge) => ({
        ...edge,
        sourcePoint: projectedById.get(edge.source),
        targetPoint: projectedById.get(edge.target),
      })),
      center: layout.center,
      width,
      height,
    };
  }

  function layoutDimensions(nodeCount) {
    return {
      width: Math.min(12000, Math.max(1000, nodeCount * 60)),
      height: Math.min(12000, Math.max(700, nodeCount * 40)),
    };
  }

  function policyAdjacency(model, rules) {
    const map = new Map(model.nodes.map((node) => [node.id, []]));
    const orderedRules = [...(rules || [])].sort(
      (left, right) =>
        Number(left.priority || 0) - Number(right.priority || 0) ||
        String(left.rel).localeCompare(String(right.rel)),
    );
    for (const rule of orderedRules) {
      for (const edge of model.edges) {
        if (edge.rel !== rule.rel) continue;
        if (rule.direction === "outbound") {
          map.get(edge.source)?.push({
            id: edge.target,
            edge,
            direction: "outbound",
            priority: Number(rule.priority || 0),
          });
        } else {
          map.get(edge.target)?.push({
            id: edge.source,
            edge,
            direction: "inbound",
            priority: Number(rule.priority || 0),
          });
        }
      }
    }
    for (const values of map.values()) {
      values.sort(
        (left, right) =>
          left.priority - right.priority ||
          left.id.localeCompare(right.id) ||
          left.edge.rel.localeCompare(right.edge.rel),
      );
    }
    return map;
  }

  function neighborhood(model, rootId, hops, rules) {
    if (!rootId) return { ids: new Set(), depth: new Map(), treeEdges: [], ghostEdges: [] };
    const graph = policyAdjacency(
      model,
      rules || [...new Set(model.edges.map((edge) => edge.rel))].flatMap((rel) => [
        { rel, direction: "outbound", priority: 0 },
        { rel, direction: "inbound", priority: 0 },
      ]),
    );
    const walked = boundedWalk(graph, rootId, hops);
    const { depth } = walked;
    const treeEdgeIds = new Set(walked.paths.map((path) => path.edgeId));
    const ids = new Set(depth.keys());
    const relevant = model.edges.filter(
      (edge) => ids.has(edge.source) && ids.has(edge.target),
    );
    return {
      ids,
      depth,
      treeEdges: relevant.filter((edge) => treeEdgeIds.has(edge.id)),
      ghostEdges: relevant.filter((edge) => !treeEdgeIds.has(edge.id)),
    };
  }

  function policyPaths(model, rootId, rules, hops) {
    if (!rootId) return [];
    const graph = policyAdjacency(model, rules);
    return boundedWalk(graph, rootId, hops).paths;
  }

  function boundedWalk(graph, rootId, hops) {
    const depth = new Map([[rootId, 0]]);
    const queue = [rootId];
    const paths = [];
    for (let cursor = 0; cursor < queue.length; cursor += 1) {
      const current = queue[cursor];
      const currentDepth = depth.get(current);
      if (currentDepth >= hops) continue;
      for (const neighbor of graph.get(current) || []) {
        if (depth.has(neighbor.id)) continue;
        depth.set(neighbor.id, currentDepth + 1);
        paths.push({
          from: current,
          to: neighbor.id,
          rel: neighbor.edge.rel,
          direction: neighbor.direction,
          depth: currentDepth + 1,
          edgeId: neighbor.edge.id,
        });
        queue.push(neighbor.id);
      }
    }
    return { depth, paths };
  }

  function radialLayout(model, rootId, width, height, rules) {
    const context = neighborhood(model, rootId, 2, rules);
    const centerX = width / 2;
    const centerY = height / 2;
    const groups = new Map();
    for (const id of context.ids) {
      const d = context.depth.get(id);
      if (!groups.has(d)) groups.set(d, []);
      groups.get(d).push(id);
    }
    const byId = new Map(model.nodes.map((node) => [node.id, node]));
    const nodes = [];
    for (const [depth, ids] of [...groups.entries()].sort((a, b) => a[0] - b[0])) {
      ids.sort();
      ids.forEach((id, index) => {
        const angle = ids.length === 1 ? 0 : (Math.PI * 2 * index) / ids.length - Math.PI / 2;
        const radius = depth * Math.min(width, height) * 0.22;
        nodes.push({
          ...byId.get(id),
          depth,
          x: centerX + Math.cos(angle) * radius,
          y: centerY + Math.sin(angle) * radius,
        });
      });
    }
    return {
      nodes,
      edges: [...context.treeEdges, ...context.ghostEdges],
      ghostEdgeIds: new Set(context.ghostEdges.map((edge) => edge.id)),
      width,
      height,
    };
  }

  function searchMatches(nodes, query) {
    const normalized = String(query || "").trim().toLowerCase();
    if (!normalized) return [];
    return nodes
      .filter((node) =>
        [node.id, node.title, node.summary, ...(node.tags || [])]
          .join(" ")
          .toLowerCase()
          .includes(normalized),
      )
      .map((node) => node.id)
      .sort();
  }

  function directionalNeighbor(nodes, currentId, direction) {
    const current = nodes.find((node) => node.id === currentId);
    if (!current) return null;
    const vector = {
      ArrowLeft: [-1, 0],
      ArrowRight: [1, 0],
      ArrowUp: [0, -1],
      ArrowDown: [0, 1],
    }[direction];
    if (!vector) return null;
    const candidates = nodes
      .filter((node) => node.id !== current.id)
      .map((node) => {
        const dx = node.x - current.x;
        const dy = node.y - current.y;
        const dot = dx * vector[0] + dy * vector[1];
        const distance = dx * dx + dy * dy;
        const magnitude = Math.sqrt(distance) || 1;
        const angle = 1 - dot / magnitude;
        return { node, dot, angle, distance };
      })
      .filter((candidate) => candidate.dot > 0)
      .sort(
        (left, right) =>
          left.angle - right.angle ||
          left.distance - right.distance ||
          left.node.id.localeCompare(right.node.id),
      );
    return candidates[0]?.node.id || null;
  }

  return {
    initialState,
    normalizeState,
    reduceState,
    mindMapRoot,
    decodeUrlState,
    encodeUrlState,
    normalizeProjection,
    deterministicLayout,
    deterministic3DLayout,
    canonicalSpatialCamera,
    projectSpatialPoint,
    projectSpatialLayout,
    layoutDimensions,
    radialLayout,
    neighborhood,
    policyPaths,
    searchMatches,
    directionalNeighbor,
    artifactHealth,
    canonicalJson,
    runWithDeadline,
  };
});
