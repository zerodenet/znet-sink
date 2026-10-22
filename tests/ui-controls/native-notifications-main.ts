import { mount } from 'svelte';
import './styles.css';
import MobileNotificationSettings from '../../src/lib/components/mobile/MobileNotificationSettings.svelte';
mount(MobileNotificationSettings, { target: document.getElementById('app')! });
