import { mount } from 'svelte';
import '../ui-controls/styles.css';
import ClientPreview from './ClientPreview.svelte';
mount(ClientPreview, { target: document.getElementById('app')! });
