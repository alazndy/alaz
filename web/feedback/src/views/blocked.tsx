export function Blocked({ email }: { email: string }) {
  return (
    <main>
      <h1>Davetli değilsin</h1>
      <p>
        <strong>{email}</strong> adresi davet listesinde yok. Geliştiriciden davet iste, ya da sağ üstten çıkıp davet edilen
        adresinle gir.
      </p>
    </main>
  );
}
