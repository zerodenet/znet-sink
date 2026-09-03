import { mount } from 'svelte';
import '../../src/app.css';
import Harness from './Harness.svelte';

mount(Harness, { target: document.getElementById('app')! });
