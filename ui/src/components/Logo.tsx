export default function Logo({ className }: { className?: string }) {
  return (
    <img
      src="/logo.svg"
      alt="MemoryPort"
      width={31}
      height={22}
      className={className}
    />
  );
}
