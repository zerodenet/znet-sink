<script lang="ts">
	import { Switch as SwitchPrimitive } from "bits-ui";
	import { cn, type WithoutChildrenOrChild } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		checked = $bindable(false),
		size = "default",
		...restProps
	}: WithoutChildrenOrChild<SwitchPrimitive.RootProps> & {
		size?: "sm" | "default";
	} = $props();
</script>

<SwitchPrimitive.Root
	bind:ref
	bind:checked
	data-slot="switch"
	data-size={size}
	class={cn(
		"data-[state=checked]:bg-success data-[state=unchecked]:bg-input focus-visible:border-ring focus-visible:ring-ring/30 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive shrink-0 rounded-full border border-foreground/10 shadow-inner focus-visible:ring-3 aria-invalid:ring-3 data-[size=default]:h-5 data-[size=default]:w-9 data-[size=sm]:h-4 data-[size=sm]:w-7 peer group/switch relative inline-flex items-center p-px transition-[background-color,border-color,box-shadow] outline-none after:absolute after:-inset-x-3 after:-inset-y-2 data-disabled:cursor-not-allowed data-disabled:opacity-45",
		className
	)}
	{...restProps}
>
	<SwitchPrimitive.Thumb
		data-slot="switch-thumb"
		class="bg-white rounded-full shadow-sm group-data-[size=default]/switch:size-4 group-data-[size=sm]/switch:size-3 data-[state=checked]:group-data-[size=default]/switch:translate-x-4 data-[state=checked]:group-data-[size=sm]/switch:translate-x-3 data-[state=unchecked]:translate-x-0 pointer-events-none block ring-0 transition-transform rtl:data-[state=checked]:-translate-x-full"
	/>
</SwitchPrimitive.Root>
