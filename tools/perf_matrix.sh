#!/usr/bin/env bash
# Runs the game through every situation we care about and records frame statistics for each.
#
#   [ONLY=regex] [BIN=path] tools/perf_matrix.sh [release|debug] [seconds per scenario] [out.tsv]
#
# Each scenario starts the game with dev aids (env vars, see main.rs), waits, and keeps the [perf] lines
# (ALAZ_PERF prints one every 5 s; the first is warm-up and dropped). Vsync is off so the numbers are the real
# cost, not the monitor's rate. Output: one TSV row per scenario.
set -u
profile=${1:-release}
secs=${2:-17}
out=${3:-perf_matrix.tsv}
bin=${BIN:-./target/$profile/alaz}
[ -x "$bin" ] || { echo "no $bin: build it first" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'pkill -x alaz 2>/dev/null; rm -rf "$tmp"' EXIT

# name | args | env (space separated KEY=VALUE)
scenarios=(
  "menu||"
  "new-world-idle|--new --seed 7|"
  "camera-flight|--new --seed 7|ALAZ_BENCH=15"
  "demo-base|--demo --seed 7|ALAZ_DEMO_IDLE=1"
  "demo-walk|--demo --seed 7|ALAZ_DEMO_MOVE=1"
  "night|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_HOUR=23"
  "rain|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_WEATHER=rain"
  "snow|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_WEATHER=snow"
  "campfire|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_FIRE=1 ALAZ_DEMO_HOUR=22"
  "forest|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_AT=forest"
  "mountain|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_AT=peak"
  "cave-inside|--demo --seed 7|ALAZ_DEMO_CAVE=in"
  "chopping|--demo --seed 7|ALAZ_DEMO_WORK=chop"
  "zoom-far|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_ZOOM=0.67"
  "top-down|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_PITCH=90"
  "panel-bag|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_TAB=bag"
  "panel-craft|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_TAB=craft"
  "panel-build|--demo --seed 7|ALAZ_DEMO_IDLE=1 ALAZ_DEMO_TAB=build"
  "mg-knap|--demo --seed 7|ALAZ_DEMO_CRAFT=sharp_stone ALAZ_DEMO_STEP=0"
  "mg-trim|--demo --seed 7|ALAZ_DEMO_CRAFT=stone_axe ALAZ_DEMO_STEP=0"
  "mg-rub|--demo --seed 7|ALAZ_DEMO_CRAFT=stone_axe ALAZ_DEMO_STEP=1"
  "mg-fit|--demo --seed 7|ALAZ_DEMO_CRAFT=stone_axe ALAZ_DEMO_STEP=4"
  "mg-tension|--demo --seed 7|ALAZ_DEMO_CRAFT=stone_axe ALAZ_DEMO_STEP=5"
  "mg-timing|--demo --seed 7|ALAZ_DEMO_CRAFT=spear ALAZ_DEMO_STEP=0"
  "mg-stack|--demo --seed 7|ALAZ_DEMO_CRAFT=log_pile ALAZ_DEMO_STEP=0"
  "creative-x1|--new --creative --seed 7|"
  "creative-x10|--new --creative --seed 7|ALAZ_TIME_SCALE=10"
  "creative-panel|--new --creative --seed 7|ALAZ_DEMO_TAB=creative"
  "big-world-32k|--new --seed 7 --size 32768|"
  "fullhd-demo|--demo --seed 7|ALAZ_DEMO_IDLE=1 WINDOW=0,0,1920,1080"
)

printf "scenario\tfps\tframe_avg_ms\tframe_p99_ms\tframe_max_ms\trender_avg_ms\trender_p99_ms\tsim_avg_ms\tchunks\tverts\trss_mb\tpanic\n" > "$out"
for row in "${scenarios[@]}"; do
  IFS='|' read -r name args envs <<< "$row"
  # ONLY=regex runs just the matching scenarios
  [[ -n "${ONLY:-}" && ! "$name" =~ ${ONLY} ]] && continue
  window=0,0,1280,800
  envv=(ALAZ_PERF=1 ALAZ_NOVSYNC=1 ALAZ_MUTE=1 ALAZ_NOCINEMA=1 ALAZ_NOFX=1)
  for e in $envs; do
    case $e in WINDOW=*) window=${e#WINDOW=} ;; *) envv+=("$e") ;; esac
  done
  log="$tmp/$name.log"
  # shellcheck disable=SC2086
  env "${envv[@]}" ALAZ_SAVE_DIR="$tmp/save" ALAZ_WINDOW="$window" "$bin" $args > "$log" 2>&1 &
  pid=$!
  rss=0
  for ((t = 0; t < secs; t++)); do
    sleep 1
    kill -0 "$pid" 2>/dev/null || break
    r=$(awk '/VmRSS/ {print int($2/1024)}' /proc/$pid/status 2>/dev/null)
    [ -n "$r" ] && [ "$r" -gt "$rss" ] && rss=$r
  done
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  panic=$(grep -c "panicked" "$log")
  # keep every [perf] line after the first; average the numbers, worst p99 and max
  grep "^\[perf\]" "$log" | tail -n +2 | awk -v name="$name" -v rss="$rss" -v panic="$panic" '
    {
      for (i = 1; i <= NF; i++) {
        if ($i == "fps") fps += $(i-1)
        if ($i == "avg" && $(i-1) == "frame") fa += $(i+1)
        if ($i == "p99" && $(i-4) == "frame") { if ($(i+1) > fp) fp = $(i+1) }
        if ($i == "max") { if ($(i+1) > fm) fm = $(i+1) }
        if ($i == "avg" && $(i-1) == "render()") ra += $(i+1)
        if ($i == "p99" && $(i-3) == "render()") { if ($(i+1) > rp) rp = $(i+1) }
        if ($i == "avg" && $(i-1) == "sim") sa += $(i+1)
        if ($i ~ /^chunks<=/) { split($i, c, "="); if (c[2] > ch) ch = c[2] }
        if ($i ~ /^verts<=/) { split($i, v, "="); if (v[2] > vt) vt = v[2] }
      }
      n++
    }
    END {
      if (n == 0) { printf "%s\t-\t-\t-\t-\t-\t-\t-\t-\t-\t%s\t%s\n", name, rss, panic; exit }
      printf "%s\t%.0f\t%.2f\t%.2f\t%.2f\t%.2f\t%.2f\t%.3f\t%d\t%d\t%s\t%s\n", name, fps/n, fa/n, fp, fm, ra/n, rp, sa/n, ch, vt, rss, panic
    }' >> "$out"
  tail -1 "$out"
done
