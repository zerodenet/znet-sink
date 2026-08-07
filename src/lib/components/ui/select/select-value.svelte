<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn, type WithoutChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: WithoutChild<SelectPrimitive.ValueProps> = $props();
</script>

<SelectPrimitive.Value
	bind:ref
	data-slot="select-value"
	class={cn("min-w-0 flex-1 truncate text-left", className)}
	{...restProps}
/>
