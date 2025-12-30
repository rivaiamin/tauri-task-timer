
import { ENCRYPTION_KEY } from '$env/static/private';
import crypto from 'crypto';

const ALGORITHM = 'aes-256-gcm';
const IV_LENGTH = 16;
const SALT_LENGTH = 64;
const TAG_LENGTH = 16;
const TAG_POSITION = SALT_LENGTH + IV_LENGTH;
const ENCRYPTED_POSITION = TAG_POSITION + TAG_LENGTH;

function getKey(): Buffer {
  return crypto.scryptSync(ENCRYPTION_KEY, 'salt', 32);
}

export function encryptWebhookUrl(text: string): string {
  const iv = crypto.randomBytes(IV_LENGTH);
  const key = getKey();
  
  const cipher = crypto.createCipheriv(ALGORITHM, key, iv);
  
  let encrypted = cipher.update(text, 'utf8');
  encrypted = Buffer.concat([encrypted, cipher.final()]);
  
  const tag = cipher.getAuthTag();
  
  return Buffer.concat([
    Buffer.from('salt'), // Salt placeholder
    iv,
    tag,
    encrypted
  ]).toString('base64');
}

export function decryptWebhookUrl(encryptedData: string): string {
  const data = Buffer.from(encryptedData, 'base64');
  
  const iv = data.slice(SALT_LENGTH, TAG_POSITION);
  const tag = data.slice(TAG_POSITION, ENCRYPTED_POSITION);
  const encrypted = data.slice(ENCRYPTED_POSITION);
  
  const key = getKey();
  const decipher = crypto.createDecipheriv(ALGORITHM, key, iv);
  
  decipher.setAuthTag(tag);
  
  let decrypted = decipher.update(encrypted);
  decrypted = Buffer.concat([decrypted, decipher.final()]);
  
  return decrypted.toString('utf8');
}