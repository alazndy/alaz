import type { FileName, Kind, Status } from './report';

export const KIND_NAMES: Record<Kind, string> = { bug: 'Hata', idea: 'İstek' };
export const STATUS_NAMES: Record<Status, string> = { new: 'Yeni', looking: 'Bakılıyor', done: 'Çözüldü', closed: 'Kapatıldı' };
export const FILE_LABELS: Record<FileName, string> = { screenshot: 'Ekran görüntüsü', log: 'Log', save: 'Kayıt dosyası' };

const date = new Intl.DateTimeFormat('tr-TR', { timeZone: 'Europe/Istanbul', day: '2-digit', month: '2-digit', year: 'numeric', hour: '2-digit', minute: '2-digit' });

export const when = (at: Date): string => date.format(at);
export const megabytes = (bytes: number): string => (bytes / 1_000_000).toLocaleString('tr-TR', { maximumFractionDigits: 1 });
