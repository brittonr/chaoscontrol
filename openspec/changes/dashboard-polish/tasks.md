## 1. Vendor JavaScript Dependencies

- [x] 1.1 Download Chart.js 4.4.7 UMD minified bundle and chartjs-plugin-annotation 3.1.0 UMD minified bundle, embed both as inline `<script>` blocks in `index.html` replacing the CDN `<script src>` tag
- [x] 1.2 Register the annotation plugin (`Chart.register(window['chartjs-plugin-annotation'])` or equivalent) after both scripts load, verify bug markers render on the coverage chart

## 2. Bugs Panel

- [x] 2.1 Add a bugs panel to the grid layout (left column, alongside assertions) with a table: bug ID, assertion message (truncated 60 chars), round, tick, faults count; show "No bugs found" when empty
- [x] 2.2 Wire `updateBugs(s)` into the `updateAll` function so the bugs table refreshes on every state change
- [x] 2.3 Add click handler on bug rows that scrolls the coverage chart into view and highlights the corresponding round marker

## 3. Config Panel

- [x] 3.1 Add a config panel to the grid layout showing: VMs, seed, mode, rounds, branches/round, ticks/branch, kernel (basename only); render from `state.config`
- [x] 3.2 Wire config panel into `updateAll` so it populates on initial load and after Started events

## 4. Assertion Exercise Gauge

- [x] 4.1 Add a progress bar element above the assertion table showing exercised/catalog_size with percentage text (e.g. "32 / 45 exercised (71%)")
- [x] 4.2 Hide the gauge when catalog_size is 0; use green accent when 100% exercised
- [x] 4.3 Wire the gauge into `updateAll` so it updates on every round completion

## 5. Assertion Table Sort

- [x] 5.1 Sort assertion_details client-side before rendering: failed first, then unexercised, then passed; alphabetical within each group

## 6. Verify End-to-End

- [x] 6.1 Build the dashboard binary with `cargo build -p chaoscontrol-dashboard` and test standalone mode against a real checkpoint directory (from a previous raft exploration run)
- [x] 6.2 Verify all panels render, bug markers appear on the coverage chart, assertion gauge shows correct numbers, config panel populated, no console errors
