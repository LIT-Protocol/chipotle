-- Dark-pool storage. Nothing here is secret: orders are stored as ciphertext
-- (the output of Lit.Actions.Encrypt against the vault PKP) plus the minimum
-- routing metadata needed to batch them. The order's side, price, and quantity
-- live only inside the ciphertext and are decrypted only inside the TEE.

create table if not exists orders (
  id          bigserial   primary key,
  epoch       bigint      not null,
  pair        text        not null,          -- e.g. "BASE/QUOTE"
  ciphertext  text        not null,          -- Lit.Actions.Encrypt output
  created_at  timestamptz not null default now(),
  settled     boolean     not null default false
);

create index if not exists orders_epoch_pair_idx
  on orders (epoch, pair)
  where not settled;

create table if not exists epochs (
  epoch        bigint      primary key,
  pair         text        not null,
  status       text        not null default 'open',  -- open | matching | settled
  clearing_px  numeric,
  settled_tx   text,
  closed_at    timestamptz
);
