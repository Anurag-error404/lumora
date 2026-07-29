# Hello Selection

A minimal Lumora plugin that demonstrates the full plugin lifecycle without any side effects.

## What it does

Logs the count and ids of all selected assets to the plugin log. Nothing is modified.

## Permissions

None required.

## Usage

1. Select one or more photos.
2. Open the **Plugins** menu in the selection toolbar.
3. Click **Hello Selection**.

The Developer page will show the logged asset ids.

## Code walkthrough

```js
export async function runAction(actionId, context) {
  lumora.log("info", `hello-selection: ${context.assetIds.length} asset(s) selected`);
  for (let i = 0; i < context.assetIds.length; i++) {
    context.reportProgress(i + 1, context.assetIds.length);
  }
  return { ok: true, message: `Logged ${context.assetIds.length} asset(s)` };
}
```

`context.reportProgress(current, total)` drives the progress bar in the host UI.
