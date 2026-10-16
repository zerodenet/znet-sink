import {mount} from 'svelte';
import './styles.css';
import LogPanel from '../../src/lib/components/core/LogPanel.svelte';
mount(LogPanel,{target:document.getElementById('app')!});
