import {DnsMigration} from './state';
import {dnsMigrationPorts} from '$lib/features/dns-migration/client';
export const dnsMigration=new DnsMigration(dnsMigrationPorts);
